//! Integration tests against a real PostgreSQL (REAS-TEST-001). `#[sqlx::test]` creates a fresh
//! database per test from `DATABASE_URL`; CI sets it, and `docs/runbooks/develop.md` says how to
//! set it locally. Migrations are applied through the store, not by the harness, so the store's
//! own path is what is tested.
use qrtrmastr::adapters::postgres::PostgresStore;
use qrtrmastr::application::migrate::Migrate;
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn migrating_twice_applies_everything_once(pool: PgPool) {
    let migrate = Migrate::new(PostgresStore::from_pool(pool));

    let first = migrate.run().await.expect("first migration run");
    assert_eq!(first.already_applied, 0);
    assert_eq!(first.applied.len(), 6, "{:?}", first.applied);
    assert!(
        first.applied[0].ends_with("raw mail"),
        "{:?}",
        first.applied
    );

    let second = migrate.run().await.expect("second migration run");
    assert!(second.applied.is_empty(), "{:?}", second.applied);
    assert_eq!(second.already_applied, 6);
}

#[sqlx::test(migrations = false)]
async fn the_schema_has_the_tables_the_mvp_reads(pool: PgPool) {
    Migrate::new(PostgresStore::from_pool(pool.clone()))
        .run()
        .await
        .expect("migrations");

    let tables: Vec<(String,)> = sqlx::query_as(
        "select table_name::text from information_schema.tables \
         where table_schema = 'public' and table_type = 'BASE TABLE' order by table_name",
    )
    .fetch_all(&pool)
    .await
    .expect("list tables");
    let names: Vec<&str> = tables.iter().map(|(name,)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "_sqlx_migrations",
            "grocery_order_lines",
            "grocery_orders",
            "list_additions",
            "order_line_anomalies",
            "parse_attempts",
            "product_aliases",
            "product_preferences",
            "products",
            "raw_messages",
        ]
    );

    let (views,): (i64,) = sqlx::query_as(
        "select count(*) from information_schema.views where table_name = 'purchases'",
    )
    .fetch_one(&pool)
    .await
    .expect("count views");
    assert_eq!(views, 1);
}

#[sqlx::test(migrations = false)]
async fn a_message_with_an_unknown_kind_is_refused_by_the_check_constraint(pool: PgPool) {
    Migrate::new(PostgresStore::from_pool(pool.clone()))
        .run()
        .await
        .expect("migrations");

    let result = sqlx::query(
        "insert into raw_messages \
           (message_key, key_kind, received_at, subject, kind, dkim_pass, source_sha256, redaction_version) \
         values ('k1', 'message_id', now(), 'subject', 'bogus', true, '\\x00', 1)",
    )
    .execute(&pool)
    .await;

    let message = result.expect_err("the insert must fail").to_string();
    assert!(message.contains("raw_messages_kind_check"), "{message}");
}

// ---------------------------------------------------------------------------------------------
// The import path end to end: raw mail in, redacted rows, orders, aliases and recommendations.
// ---------------------------------------------------------------------------------------------

use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::fixture::ExportFixture;
use qrtrmastr::application::import::ImportMail;
use qrtrmastr::application::parse::ParsePending;
use qrtrmastr::application::ports::{
    AliasId, AttemptRecord, GroceryStore, InsertOutcome, MailSource, MailStream, NewRawMessage,
    OrderWrite, ParseAttempt, ParseFinish, ParserTag, PendingMessage, RawMail, RawMessageStore,
    ReceiptParsing, ReceivedPages, StoreError,
};
use qrtrmastr::application::recommend::{Due, DueOptions};
use qrtrmastr::domain::ids::{IdError, MessageKey};
use qrtrmastr::domain::recommend::{Status, Thresholds};
use qrtrmastr::domain::states::{
    KeyKind, MessageKind, ParseOutcome, ParseState, Platform, SenderCheck,
};
use std::collections::HashSet;

const FIXTURE: &str = include_str!("fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

/// A receipt mail for `order` delivered on `date`, built from the synthetic fixture.
fn receipt_mail(order: &str, date: &str, message_id: &str) -> RawMail {
    let html = FIXTURE
        .replace("12345678901", order)
        .replace("Tuesday, March 3, 2026", date);
    let bytes = format!(
        "From: FreshDirect <service@freshdirect.com>\r\nTo: pat.example@example.com\r\n\
         Subject: Your FreshDirect order is in the works! Here's your receipt\r\n\
         Date: Tue, 3 Mar 2026 06:15:00 -0500\r\nMessage-ID: <{message_id}@mail.example>\r\n\
         Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n\
         Content-Type: text/html; charset=utf-8\r\n\r\n{html}"
    );
    RawMail {
        bytes: bytes.into_bytes(),
    }
}

/// Inserts a raw message row with the given key columns, past the store: what a row from an
/// older version, or a row that someone changed by hand, looks like.
async fn insert_raw_row(pool: &PgPool, kind: &str, text: &str, hash: &[u8]) -> i64 {
    let (id,): (i64,) = sqlx::query_as(
        "insert into raw_messages \
           (message_key, key_kind, received_at, subject, kind, dkim_pass, body_html, \
            source_sha256, redaction_version) \
         values ($1, $2, now(), 'subject', 'receipt', true, '<p>stored</p>', $3, 1) \
         returning id",
    )
    .bind(text)
    .bind(kind)
    .bind(hash)
    .fetch_one(pool)
    .await
    .expect("the row is inserted");
    id
}

#[sqlx::test(migrations = false)]
async fn a_stored_message_is_found_by_its_key_and_only_under_its_kind(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    assert_eq!(store.known_keys().await.expect("the keys"), HashSet::new());
    assert_eq!(store.newest_received_at().await.expect("the newest"), None);
    let report = ImportMail::new(
        Mails(vec![receipt_mail(
            "12345678901",
            "Tuesday, March 3, 2026",
            "r1",
        )]),
        reader(),
        store.clone(),
    )
    .run()
    .await
    .expect("import");
    assert_eq!(report.inserted, 1);
    // The Date header of the mail, 06:15 at five hours behind UTC.
    assert_eq!(
        store.newest_received_at().await.expect("the newest"),
        Some("2026-03-03T11:15:00Z".parse().expect("a timestamp"))
    );
    let digest = "0123456789abcdef".repeat(4);
    insert_raw_row(&pool, "sha256", &digest, b"hash of the second row").await;

    let from_header = MessageKey::from_message_id("<r1@mail.example>").expect("a key");
    let from_bytes = MessageKey::from_parts(KeyKind::Sha256, &digest).expect("a key");
    assert_eq!(
        store.known_keys().await.expect("the keys"),
        HashSet::from([from_header.clone(), from_bytes.clone()])
    );

    // The use case of `qm fixture add` finds the row, and none for a key that is not stored.
    let export = ExportFixture::new(store.clone());
    let exported = export
        .run(&from_header)
        .await
        .expect("the lookup")
        .expect("the imported message");
    assert_eq!(exported.key, from_header);
    // A row of an import of today holds no page.
    assert_eq!(exported.body_html, None);
    let absent = MessageKey::from_message_id("<absent@mail.example>").expect("a key");
    assert!(export.run(&absent).await.expect("the lookup").is_none());

    let body = store
        .body_by_key(&from_header)
        .await
        .expect("the lookup")
        .expect("the imported message");
    assert_eq!(body.key, from_header);
    assert_eq!(body.platform, Platform::Freshdirect);
    assert_eq!(body.body_html, None);
    let body = store
        .body_by_key(&from_bytes)
        .await
        .expect("the lookup")
        .expect("the row with the digest");
    assert_eq!(body.key, from_bytes);
    assert_eq!(body.body_html.as_deref(), Some("<p>stored</p>"));

    // A text that is stored under the other kind is not that key, and a text that is not
    // stored is no key at all.
    let digest_as_header = MessageKey::from_parts(KeyKind::MessageId, &digest).expect("a key");
    assert_eq!(
        store
            .body_by_key(&digest_as_header)
            .await
            .expect("the lookup"),
        None
    );
    let absent = MessageKey::from_message_id("<r2@mail.example>").expect("a key");
    assert_eq!(store.body_by_key(&absent).await.expect("the lookup"), None);

    // The text is unique in the table whatever its kind, so the insert of the same text
    // under the other kind stores nothing and says that the message is known.
    let outcome = store
        .insert(&NewRawMessage {
            platform: Platform::Freshdirect,
            key: digest_as_header,
            received_at: jiff::Timestamp::UNIX_EPOCH,
            kind: MessageKind::Receipt,
            dkim_pass: true,
            source_sha256: [7; 32],
        })
        .await
        .expect("the insert");
    assert_eq!(outcome, InsertOutcome::AlreadyKnown);
    assert_eq!(count(&pool, "select count(*) from raw_messages").await, 2);
}

/// The synthetic receipt with the final prices of its first two lines replaced by `price`.
fn receipt_mail_with_two_prices(order: &str, message_id: &str, price: &str) -> RawMail {
    let mail = receipt_mail(order, "Tuesday, March 3, 2026", message_id);
    let text = String::from_utf8(mail.bytes).expect("the mail is text");
    let changed = text
        .replace("<td>$17.37</td>", &format!("<td>{price}</td>"))
        .replace("<td>$6.49</td>", &format!("<td>{price}</td>"));
    assert_eq!(changed.matches(price).count(), 2, "two prices are replaced");
    RawMail {
        bytes: changed.into_bytes(),
    }
}

#[sqlx::test(migrations = false)]
async fn a_receipt_whose_amounts_cannot_be_added_is_a_failed_attempt_and_no_order(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // Two line prices at the end of the range of an amount: each is an amount, and their sum
    // is none.
    let mails = Mails(vec![
        receipt_mail_with_two_prices("12345678901", "big", "$99,999,999.99"),
        // The control: a receipt beside it is parsed and stored.
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "small"),
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    assert_eq!(imported.inserted, 2);

    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!(
        (parsed.pending, parsed.parsed, parsed.partial, parsed.failed),
        (2, 1, 0, 1)
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_orders where order_number = '12345678902'"
        )
        .await,
        1
    );
    assert_eq!(count(&pool, "select count(*) from grocery_orders").await, 1);
    let (outcome, error): (String, Option<String>) = sqlx::query_as(
        "select a.outcome, a.error from parse_attempts a \
         join raw_messages m on m.id = a.raw_message_id \
         where m.message_key = 'big@mail.example'",
    )
    .fetch_one(&pool)
    .await
    .expect("the attempt of the first receipt");
    assert_eq!(outcome, "failed");
    assert_eq!(
        error.as_deref(),
        Some(
            "the amounts of the receipt cannot be checked: an amount of money is outside the range of 99,999,999.99"
        )
    );
    // A run that did not read the mail again has no page for the message. The message
    // waits, and nothing is written for it.
    let again = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &ReceivedPages::default())
        .await
        .expect("parse");
    assert_eq!(
        (again.pending, again.not_in_this_run, again.failed),
        (1, 1, 0)
    );
    // The two attempts of the first run: one for each receipt.
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 2);
    // A run that reads the mail again gives it a new attempt, from the mail.
    let retried = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!((retried.pending, retried.failed), (1, 1));
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 3);
}

#[sqlx::test(migrations = false)]
async fn a_price_that_no_column_holds_does_not_stop_the_messages_after_it(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // One cent more than a `numeric(10,2)` column holds. Before 2026-11-21 such a price was
    // an amount, the insert of its line failed in the database, and each run of the parse
    // step stopped at this message with that error: no later message was parsed.
    let mails = Mails(vec![
        receipt_mail_with_two_prices("12345678901", "too-big", "$100,000,000.00"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "after"),
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");

    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("the run goes on after the receipt with the price");
    assert_eq!(
        (parsed.pending, parsed.parsed, parsed.partial, parsed.failed),
        (2, 1, 1, 0)
    );
    // The message after it has its order, and nothing waits.
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_orders where order_number = '12345678902'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'fetched'"
        )
        .await,
        0
    );
    // No stored price is the price that is no amount.
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_order_lines where final_price >= 99999999.99"
        )
        .await,
        0
    );
    let outcome: (String, Option<String>, i32) = sqlx::query_as(
        "select a.outcome, a.error, a.anomaly_count from parse_attempts a \
         join raw_messages m on m.id = a.raw_message_id \
         where m.message_key = 'too-big@mail.example'",
    )
    .fetch_one(&pool)
    .await
    .expect("the attempt of the first receipt");
    // Its two rows with the price are rows that the parser cannot read, and the lines that
    // are left do not sum to the subtotal: an order that does not reconcile, with anomalies.
    assert_eq!(outcome, ("partial".to_owned(), None, 4));
}

/// A text of `length` letters that does not compress: the store keeps a long text in a
/// shorter form when it repeats, and then an index takes a longer one.
fn letters_that_do_not_repeat(length: usize) -> String {
    let mut state: u32 = 0x2545_f491;
    (0..length)
        .map(|_| {
            // One step of a small generator; the same letters on each run.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let letter = usize::try_from(state % 26).expect("below 26");
            char::from(*b"abcdefghijklmnopqrstuvwxyz".get(letter).expect("a letter"))
        })
        .collect()
}

#[sqlx::test(migrations = false)]
async fn a_product_name_that_no_index_holds_does_not_stop_the_messages_after_it(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // Before 2026-11-21 the line with this name was read, the unique index of the aliases
    // refused its key, and each run of the parse step stopped at this message with the
    // database error: no later message was parsed.
    let name = letters_that_do_not_repeat(3000);
    let mail = receipt_mail("12345678901", "Tuesday, March 3, 2026", "long-name");
    let text = String::from_utf8(mail.bytes).expect("the mail is text");
    let changed = text.replace("Beefsteak Tomatoes, Large", &name);
    assert!(changed.contains(&name), "the name is replaced");
    let mails = Mails(vec![
        RawMail {
            bytes: changed.into_bytes(),
        },
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "after"),
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");

    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("the run goes on after the receipt with the name");
    // The row with the name is a row that the parser cannot read: an order of five lines
    // that do not sum to the subtotal. The message after it has its order of six.
    assert_eq!(
        (parsed.pending, parsed.parsed, parsed.partial, parsed.failed),
        (2, 1, 1, 0)
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_order_lines where order_number = '12345678901'"
        )
        .await,
        5
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_order_lines where order_number = '12345678902'"
        )
        .await,
        6
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'fetched'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from product_aliases where length(name_norm) > 1024"
        )
        .await,
        0
    );
    // The anomaly says the limit and not the name.
    assert_eq!(
        count(
            &pool,
            "select count(*) from order_line_anomalies \
             where reason = 'product name longer than 1024 bytes'"
        )
        .await,
        1
    );
}

/// A parser that reads the synthetic receipt and gives its first line a name that is too long
/// for an alias: what a parser without the limit of its own would give. The name for the
/// first order is longer than any index holds. The name for each other order is 96 bytes on
/// the receipt and more than 1,024 after it is normalized, which the index would hold: the
/// limit is on the normalized name.
struct ParserWithoutTheLimit;

impl ReceiptParsing for ParserWithoutTheLimit {
    fn parse(&self, html: &str) -> ParseAttempt {
        let mut attempt = Registry::default().parse(html);
        if let ParseAttempt::Parsed { receipt, .. } = &mut attempt
            && let Some(line) = receipt.lines.first_mut()
        {
            line.raw_name = if html.contains("12345678901") {
                letters_that_do_not_repeat(3000)
            } else {
                "\u{fdfa}".repeat(32)
            };
        }
        attempt
    }
}

#[sqlx::test(migrations = false)]
async fn a_name_too_long_for_an_alias_from_any_parser_is_a_failed_attempt(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "r2"),
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");

    let parsed = ParsePending::new(store.clone(), ParserWithoutTheLimit)
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("the use case has the limit too");
    assert_eq!(
        (parsed.pending, parsed.parsed, parsed.partial, parsed.failed),
        (2, 0, 0, 2)
    );
    // No order, no alias and no product is left from a failed attempt, and nothing waits.
    for (table, sql) in [
        ("grocery_orders", "select count(*) from grocery_orders"),
        ("product_aliases", "select count(*) from product_aliases"),
        ("products", "select count(*) from products"),
    ] {
        assert_eq!(count(&pool, sql).await, 0, "{table}");
    }
    assert_eq!(
        count(
            &pool,
            "select count(*) from parse_attempts \
             where outcome = 'failed' and error = 'a product name is longer than 1024 bytes'"
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'fetched'"
        )
        .await,
        0
    );
    // The control: the real parser reads the same two messages into two orders. The
    // messages are failed now, so they are imported into a second, empty state through a
    // fresh pair of mails.
    let more = Mails(vec![receipt_mail(
        "12345678903",
        "Tuesday, March 17, 2026",
        "r3",
    )]);
    let imported = ImportMail::new(more, reader(), store.clone())
        .run()
        .await
        .expect("import");
    let real = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    // The two messages of the first run failed and wait for their mail; this run has the
    // page of the third only.
    assert_eq!((real.pending, real.not_in_this_run, real.parsed), (3, 2, 1));
}

#[sqlx::test(migrations = false)]
async fn a_message_id_that_no_index_holds_is_stored_under_the_hash(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // Before 2026-11-21 this header was the key, the unique index of the keys refused it,
    // and the import stopped at this mail on each run.
    let long = letters_that_do_not_repeat(3000);
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", &long),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "after"),
    ]);
    let first = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("the import goes on after the mail with the header");
    assert_eq!((first.read, first.inserted), (2, 2), "{first:?}");
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages \
             where key_kind = 'sha256' and message_key = encode(source_sha256, 'hex')"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages \
             where key_kind = 'message_id' and message_key = 'after@mail.example'"
        )
        .await,
        1
    );
    // The same mails again: each is known by its key, also the one under the hash.
    let second = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    assert_eq!(
        (second.inserted, second.already_known),
        (0, 2),
        "{second:?}"
    );
}

#[sqlx::test(migrations = false)]
async fn a_mail_with_a_nul_character_in_its_body_does_not_stop_the_import(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // PostgreSQL holds no NUL character in a text. Before 2026-11-21 the insert of such a
    // mail failed and the import stopped at it on each run. Until 2026-11-24 the mail was
    // refused. Now no text of a mail is stored, so the character has no place to go.
    let with_nul = |message_id: &str, body: &str| {
        let mail = receipt_mail("12345678901", "Tuesday, March 3, 2026", message_id);
        let text = String::from_utf8(mail.bytes).expect("the mail is text");
        let changed = text.replace("<body", &format!("{body}<body"));
        assert!(changed.contains(body), "the body is changed");
        RawMail {
            bytes: changed.into_bytes(),
        }
    };
    let mails = Mails(vec![
        with_nul("nul-byte", "<p>a\0b</p>"),
        with_nul("nul-entity", "<p>a&#0;b</p>"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "after"),
    ]);
    let first = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("the import goes on after the two mails");
    assert_eq!(first.read, 3);
    assert_eq!((first.inserted, first.malformed.len()), (3, 0), "{first:?}");
    assert_eq!(count(&pool, "select count(*) from raw_messages").await, 3);
    // A second run knows the three.
    let second = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    assert_eq!(
        (second.inserted, second.already_known),
        (0, 3),
        "{second:?}"
    );
}

#[sqlx::test(migrations = false)]
async fn a_row_of_before_with_a_stored_page_is_parsed_from_it_once(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // A row as the program stored it until 2026-11-24: never parsed, with its redacted
    // page. No import of this run read its mail.
    insert_raw_row(&pool, "message_id", "old@mail.example", b"an older row").await;
    let none = ReceivedPages::default();
    let first = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &none)
        .await
        .expect("parse");
    // The stored page is read: `<p>stored</p>` is a layout that no parser knows.
    assert_eq!(
        (first.pending, first.unsupported, first.not_in_this_run),
        (1, 1, 0)
    );
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 1);
    // After that attempt the row waits for its mail, as a row of today does: the stored
    // page is not read on each run.
    let second = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &none)
        .await
        .expect("parse");
    assert_eq!(
        (second.pending, second.unsupported, second.not_in_this_run),
        (1, 0, 1)
    );
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 1);
}

#[sqlx::test(migrations = false)]
async fn a_stored_key_that_does_not_fit_its_kind_is_an_invalid_row(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // The control: a row whose key fits its kind is read.
    insert_raw_row(&pool, "message_id", "good@mail.example", b"first").await;
    assert_eq!(store.known_keys().await.expect("the keys").len(), 1);
    assert_eq!(
        store
            .pending(Platform::Freshdirect)
            .await
            .expect("the pending messages")
            .len(),
        1
    );

    // The check constraint admits the kind; only the domain knows what a digest is.
    let id = insert_raw_row(&pool, "sha256", "not-a-digest", b"second").await;
    let invalid = StoreError::InvalidRow {
        table: "raw_messages",
        id,
        reason: IdError::NotSha256.to_string(),
    };
    assert_eq!(
        store
            .known_keys()
            .await
            .expect_err("the row is invalid")
            .to_string(),
        invalid.to_string()
    );
    assert_eq!(
        store
            .pending(Platform::Freshdirect)
            .await
            .expect_err("the row is invalid")
            .to_string(),
        invalid.to_string()
    );
    // The reason names the rule and not the stored text.
    assert!(!invalid.to_string().contains("not-a-digest"));
}

#[sqlx::test(migrations = false)]
async fn a_long_key_that_an_older_version_stored_does_not_stop_a_run(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // A key from a mail is at most 998 bytes since 2026-11-21. A version before that stored
    // a longer `Message-ID` when the index could hold it. Such a row is still a row.
    let long = letters_that_do_not_repeat(2000);
    insert_raw_row(&pool, "message_id", &long, b"older").await;
    let keys = store.known_keys().await.expect("the keys are read");
    assert_eq!(keys.len(), 1);
    assert!(keys.iter().all(|key| key.as_str() == long));
    assert_eq!(
        store
            .pending(Platform::Freshdirect)
            .await
            .expect("the pending messages are read")
            .len(),
        1
    );
}

#[derive(Clone)]
struct Mails(Vec<RawMail>);

impl MailSource for Mails {
    fn messages(&self) -> MailStream {
        let (tx, rx) = tokio::sync::mpsc::channel(self.0.len().max(1));
        for mail in &self.0 {
            tx.try_send(Ok(mail.clone()))
                .expect("the channel holds every mail");
        }
        rx
    }
}

fn reader() -> PlatformMail {
    PlatformMail::new(vec![
        ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
        ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
    ])
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(sql).fetch_one(pool).await.expect(sql);
    n
}

#[sqlx::test(migrations = false)]
async fn importing_the_same_export_twice_stores_everything_once(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "r2"),
        receipt_mail("12345678903", "Tuesday, March 17, 2026", "r3"),
    ]);

    let first = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    assert_eq!((first.read, first.inserted, first.already_known), (3, 3, 0));
    assert!(first.redaction_failures.is_empty(), "{first:?}");
    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &first.pages)
        .await
        .expect("parse");
    assert_eq!(
        (parsed.pending, parsed.parsed, parsed.partial, parsed.failed),
        (3, 3, 0, 0)
    );
    assert_eq!(parsed.anomalies, 3, "one thank-you row per receipt");

    assert_eq!(count(&pool, "select count(*) from raw_messages").await, 3);
    assert_eq!(count(&pool, "select count(*) from grocery_orders").await, 3);
    assert_eq!(
        count(&pool, "select count(*) from grocery_order_lines").await,
        18
    );
    assert_eq!(
        count(&pool, "select count(*) from products").await,
        6,
        "the same six products across three orders"
    );
    assert_eq!(
        count(&pool, "select count(*) from product_aliases").await,
        6
    );
    assert_eq!(
        count(&pool, "select count(*) from order_line_anomalies").await,
        3
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'parsed' and version = 1"
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from grocery_orders where reconciled"
        )
        .await,
        3
    );

    // Nothing that a mail says reached the table of the messages: no row has a subject or
    // a body, and each row says so with the redaction version 0. The receipts of the
    // fixture hold the name, the address, the phone number and the card of a made-up person.
    assert!(count(&pool, "select count(*) from raw_messages").await > 0);
    let with_text = count(
        &pool,
        "select count(*) from raw_messages \
         where subject <> '' or body_html is not null or body_text is not null \
            or redaction_version <> 0",
    )
    .await;
    assert_eq!(with_text, 0);
    // And what the parser extracted holds none of it: the lines are products.
    let personal = count(
        &pool,
        "select count(*) from grocery_order_lines \
         where raw_name ilike '%Imaginary Street%' or raw_name ilike '%555-0199%' \
            or raw_name ilike '%pat.example@%' or raw_name ilike '%Pat Example%'",
    )
    .await;
    assert_eq!(personal, 0);
    // A row of a receipt that no rule of the parser read is text of the mail as it came. Its
    // place and the reason are stored, and no cell of it. The fixture has such rows.
    assert!(count(&pool, "select count(*) from order_line_anomalies").await > 0);
    assert_eq!(
        count(
            &pool,
            "select count(*) from order_line_anomalies where raw_cells <> '[]'::jsonb"
        )
        .await,
        0
    );

    let second = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("second import");
    assert_eq!(
        (second.read, second.inserted, second.already_known),
        (3, 0, 3)
    );
    let parsed_again = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &second.pages)
        .await
        .expect("parse again");
    assert_eq!(parsed_again.pending, 0);
    assert_eq!(
        count(&pool, "select count(*) from grocery_order_lines").await,
        18
    );
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 3);
}

#[sqlx::test(migrations = false)]
async fn the_gap_rule_reads_the_stored_purchases(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "r2"),
        receipt_mail("12345678903", "Tuesday, March 17, 2026", "r3"),
    ]);
    let imported = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");

    let due = Due::new(store, Thresholds::default());
    let as_of = jiff::civil::date(2026, 3, 27);
    let report = due.run(as_of, DueOptions::default()).await.expect("due");
    assert_eq!(report.orders, 3);
    assert_eq!(report.order_cadence_days, Some(7));
    // Every product was bought weekly for three weeks; ten days after the last order all six are
    // overdue by three days.
    assert_eq!(report.rows.len(), 6, "{report:?}");
    for row in &report.rows {
        assert_eq!(
            row.recommendation.status,
            Status::Overdue { days_overdue: 3 }
        );
        assert_eq!(row.recommendation.usual_gap_days, Some(7));
        assert_eq!(row.recommendation.times_bought, 3);
    }
    let milk = report
        .rows
        .iter()
        .find(|r| r.product.display_name.starts_with("Lactaid"))
        .expect("milk is a product");
    assert_eq!(milk.product.size_text.as_deref(), Some("1/2 gallon"));
    assert_eq!(milk.product.department.as_deref(), Some("Dairy"));
    assert_eq!(milk.recommendation.typical_quantity.to_string(), "3");

    // The simple rule finds nothing: every product is in the last order.
    let simple = Due::new(PostgresStore::from_pool(pool), Thresholds::default())
        .run_simple()
        .await
        .expect("simple");
    assert!(simple.rows.is_empty());
}

#[sqlx::test(migrations = false)]
async fn the_simple_rule_names_the_product_that_the_last_order_lacks(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // Three orders. The last one has another kind of tomato in the place of the usual one,
    // so the usual one is in two of three orders and not in the last.
    let last = receipt_mail("12345678903", "Tuesday, March 17, 2026", "r3");
    let text = String::from_utf8(last.bytes).expect("the mail is text");
    assert!(text.contains("Beefsteak Tomatoes, Large"));
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "r2"),
        RawMail {
            bytes: text
                .replace("Beefsteak Tomatoes, Large", "Plum Tomatoes")
                .into_bytes(),
        },
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!((parsed.pending, parsed.parsed), (3, 3));
    // The unit of a unit price is stored as one of two texts.
    let units: Vec<(String,)> = sqlx::query_as(
        "select distinct unit_price_per from grocery_order_lines \
         where unit_price_per is not null order by 1",
    )
    .fetch_all(&pool)
    .await
    .expect("the units");
    assert_eq!(units, vec![("ea".to_owned(),), ("lb".to_owned(),)]);

    let simple = Due::new(store, Thresholds::default())
        .run_simple()
        .await
        .expect("simple");
    assert_eq!(simple.orders, 3);
    assert_eq!(simple.rows.len(), 1, "{:?}", simple.rows);
    let row = &simple.rows[0];
    // The label is the one of the product of the row, among the labels of all products.
    assert_eq!(row.product.display_name, "Beefsteak Tomatoes, Large");
    assert_eq!(row.product.id, row.recommendation.product);
    assert_eq!(
        (
            row.recommendation.orders_with_product,
            row.recommendation.orders_total
        ),
        (2, 3)
    );
}

#[sqlx::test(migrations = false)]
async fn a_muted_product_is_in_no_report_and_each_row_has_the_label_of_its_product(pool: PgPool) {
    use qrtrmastr::application::recommend::HabitOptions;

    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // Three orders; the last has another tomato, so the simple rule has one row.
    let last = receipt_mail("12345678903", "Tuesday, March 17, 2026", "r3");
    let text = String::from_utf8(last.bytes).expect("the mail is text");
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        receipt_mail("12345678902", "Tuesday, March 10, 2026", "r2"),
        RawMail {
            bytes: text
                .replace("Beefsteak Tomatoes, Large", "Plum Tomatoes")
                .into_bytes(),
        },
    ]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");

    let due = Due::new(store, Thresholds::default());
    let as_of = jiff::civil::date(2026, 3, 27);
    let options = HabitOptions {
        extras: 20,
        remember: 5,
        seed: 1,
    };
    // The names of the rows of each report. Each row carries the label of its own product.
    let gap_names = || async {
        let report = due.run(as_of, DueOptions::default()).await.expect("due");
        for row in &report.rows {
            assert_eq!(row.product.id, row.recommendation.product);
        }
        let mut names: Vec<String> = report
            .rows
            .iter()
            .map(|row| row.product.display_name.clone())
            .collect();
        names.sort();
        names
    };
    let habit_names = || async {
        let report = due.run_habit(options, as_of).await.expect("habit");
        let rows: Vec<_> = report
            .essentials
            .iter()
            .chain(&report.extras)
            .chain(&report.stocked)
            .collect();
        for row in &rows {
            assert_eq!(row.product.id, row.recommendation.product);
        }
        let mut names: Vec<String> = rows
            .iter()
            .map(|row| row.product.display_name.clone())
            .collect();
        names.sort();
        names
    };
    let simple_names = || async {
        let report = due.run_simple().await.expect("simple");
        report
            .rows
            .iter()
            .map(|row| row.product.display_name.clone())
            .collect::<Vec<String>>()
    };

    let gap_before = gap_names().await;
    let habit_before = habit_names().await;
    // The habit rule lists seven products: the six of the receipt and the other tomato. The
    // gap rule leaves out the two tomatoes, which were bought twice and once.
    assert_eq!(habit_before.len(), 7, "{habit_before:?}");
    assert_eq!(gap_before.len(), 5, "{gap_before:?}");
    assert!(gap_before.iter().any(|name| name.starts_with("Lactaid")));
    assert!(
        habit_before
            .iter()
            .any(|name| name == "Beefsteak Tomatoes, Large")
    );
    assert_eq!(simple_names().await, ["Beefsteak Tomatoes, Large"]);

    // Mute the milk and the tomato. A row that is there and not muted changes nothing.
    let muted = sqlx::query(
        "insert into product_preferences (product_id, muted) \
         select id, display_name not like 'Plum%' from products \
         where display_name like 'Lactaid%' or display_name like '%Tomatoes%'",
    )
    .execute(&pool)
    .await
    .expect("the preferences");
    assert_eq!(muted.rows_affected(), 3);

    let is_muted =
        |name: &String| name.starts_with("Lactaid") || name == "Beefsteak Tomatoes, Large";
    let gap_after: Vec<String> = gap_before
        .iter()
        .filter(|name| !is_muted(name))
        .cloned()
        .collect();
    let habit_after: Vec<String> = habit_before
        .iter()
        .filter(|name| !is_muted(name))
        .cloned()
        .collect();
    assert_eq!((gap_after.len(), habit_after.len()), (4, 5));
    assert_eq!(gap_names().await, gap_after);
    assert_eq!(habit_names().await, habit_after);
    assert!(simple_names().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_muted_product_is_not_among_the_reminders(pool: PgPool) {
    use qrtrmastr::application::recommend::HabitOptions;

    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    // 24 weekly orders. The first three have a tomato that the 21 after them do not have: it
    // was bought three times and is in none of the latest 20, so it is a reminder.
    let first_day = jiff::civil::date(2025, 10, 7);
    let mails: Vec<RawMail> = (0..24i64)
        .map(|week| {
            let day = first_day
                .checked_add(jiff::Span::new().weeks(week))
                .expect("a day");
            let mail = receipt_mail(
                &format!("123456789{week:02}"),
                &day.strftime("%A, %B %-d, %Y").to_string(),
                &format!("week-{week}"),
            );
            let text = String::from_utf8(mail.bytes).expect("the mail is text");
            let name = if week < 3 {
                "Plum Tomatoes"
            } else {
                "Beefsteak Tomatoes, Large"
            };
            RawMail {
                bytes: text.replace("Beefsteak Tomatoes, Large", name).into_bytes(),
            }
        })
        .collect();
    let imported = ImportMail::new(Mails(mails), reader(), store.clone())
        .run()
        .await
        .expect("import");
    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!((parsed.pending, parsed.parsed), (24, 24), "{parsed:?}");

    let due = Due::new(store, Thresholds::default());
    let options = HabitOptions {
        extras: 20,
        remember: 5,
        seed: 1,
    };
    let reminders = || async {
        let report = due
            .run_habit(options, jiff::civil::date(2026, 3, 20))
            .await
            .expect("habit");
        for row in &report.remember {
            assert_eq!(row.product.id, row.reminder.product);
        }
        report
            .remember
            .iter()
            .map(|row| row.product.display_name.clone())
            .collect::<Vec<String>>()
    };
    assert_eq!(reminders().await, ["Plum Tomatoes"]);
    sqlx::query(
        "insert into product_preferences (product_id, muted) \
         select id, true from products where display_name = 'Plum Tomatoes'",
    )
    .execute(&pool)
    .await
    .expect("the preference");
    assert!(reminders().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_stale_version_changes_no_row(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![receipt_mail(
        "12345678901",
        "Tuesday, March 3, 2026",
        "r1",
    )]);
    ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    let pending = store.pending(Platform::Freshdirect).await.expect("pending");
    assert_eq!(pending.len(), 1);
    let message = &pending[0];

    let stale = ParseFinish {
        message: message.id,
        expected_state: ParseState::Fetched,
        expected_version: message
            .version
            .checked_add(1)
            .expect("a version above the stored one"),
        next_state: ParseState::Failed,
        attempt: AttemptRecord {
            parser: ParserTag {
                name: "test",
                version: 1,
            },
            outcome: ParseOutcome::Failed,
            error: Some("stale".to_owned()),
            anomaly_count: 0,
        },
        order: None,
    };
    let result = store.finish_parse(&stale).await;
    assert!(
        matches!(result, Err(StoreError::Conflict { .. })),
        "{result:?}"
    );
    assert_eq!(
        count(&pool, "select count(*) from parse_attempts").await,
        0,
        "the attempt row rolled back too"
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'fetched' and version = 0"
        )
        .await,
        1
    );
}

/// The record of an attempt that failed, for a message as a writer read it.
fn failed_attempt(message: &PendingMessage, error: &str) -> ParseFinish<'static> {
    ParseFinish {
        message: message.id,
        expected_state: ParseState::Fetched,
        expected_version: message.version,
        next_state: ParseState::Failed,
        attempt: AttemptRecord {
            parser: ParserTag {
                name: "test",
                version: 1,
            },
            outcome: ParseOutcome::Failed,
            error: Some(error.to_owned()),
            anomaly_count: 0,
        },
        order: None,
    }
}

#[sqlx::test(migrations = false)]
async fn of_two_writers_that_read_one_message_exactly_one_changes_it(pool: PgPool) {
    // REAS-STATE-003: the expected version is a fencing token. Two writers read the same row
    // and write at the same time, each on its own connection; one changes the row and the
    // other gets a conflict and leaves nothing.
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(
        (1..=8)
            .map(|n| {
                receipt_mail(
                    &format!("1234567890{n}"),
                    "Tuesday, March 3, 2026",
                    &format!("race-{n}"),
                )
            })
            .collect(),
    );
    ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    // Each writer reads the messages for itself, before the other one writes.
    let read_by_a = store.pending(Platform::Freshdirect).await.expect("pending");
    let read_by_b = store.pending(Platform::Freshdirect).await.expect("pending");
    assert_eq!((read_by_a.len(), read_by_b.len()), (8, 8));

    let mut winners = Vec::new();
    for (for_a, for_b) in read_by_a.iter().zip(&read_by_b) {
        assert_eq!((for_a.id, for_a.version), (for_b.id, for_b.version));
        let write_a = failed_attempt(for_a, "writer a");
        let write_b = failed_attempt(for_b, "writer b");
        let (a, b) = tokio::join!(store.finish_parse(&write_a), store.finish_parse(&write_b));
        let (winner, loser) = match (&a, &b) {
            (Ok(()), Err(error)) => ("writer a", error),
            (Err(error), Ok(())) => ("writer b", error),
            other => panic!("not exactly one writer changed the message: {other:?}"),
        };
        assert!(
            matches!(loser, StoreError::Conflict { version, .. } if *version == for_a.version),
            "{loser:?}"
        );
        // The row of the attempt of the writer that lost is rolled back: one attempt, the
        // winner's, and one step of the version.
        let attempts: Vec<(Option<String>,)> =
            sqlx::query_as("select error from parse_attempts where raw_message_id = $1")
                .bind(for_a.id.get())
                .fetch_all(&pool)
                .await
                .expect("the attempts");
        assert_eq!(attempts, vec![(Some(winner.to_owned()),)]);
        winners.push(winner);
        let state: (String, i32) =
            sqlx::query_as("select parse_state, version from raw_messages where id = $1")
                .bind(for_a.id.get())
                .fetch_one(&pool)
                .await
                .expect("the message");
        assert_eq!(
            state,
            (
                "failed".to_owned(),
                for_a.version.checked_add(1).expect("a version")
            )
        );
    }
    assert_eq!(winners.len(), 8, "{winners:?}");
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 8);
    // The state is part of the token too: a writer that expects another state than the row
    // has, with the version that the row has, changes nothing.
    let now: (i32,) = sqlx::query_as("select version from raw_messages where id = $1")
        .bind(read_by_a[0].id.get())
        .fetch_one(&pool)
        .await
        .expect("the version");
    let mut other_state = failed_attempt(&read_by_a[0], "the right version, another state");
    other_state.expected_version = now.0;
    let result = store.finish_parse(&other_state).await;
    assert!(
        matches!(result, Err(StoreError::Conflict { .. })),
        "{result:?}"
    );
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 8);
    // A writer that comes later with what it read before is refused too.
    let late = failed_attempt(&read_by_a[0], "a late writer");
    let result = store.finish_parse(&late).await;
    assert!(
        matches!(result, Err(StoreError::Conflict { .. })),
        "{result:?}"
    );
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 8);
}

#[sqlx::test(migrations = false)]
async fn a_write_that_fails_late_in_the_transaction_leaves_nothing_of_it(pool: PgPool) {
    // REAS-DATA-002: the attempt, the order, its lines and the state of the message are one
    // transaction. Here the version is right and the order is written; the first line then
    // names an alias that does not exist, which the database refuses.
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![receipt_mail(
        "12345678901",
        "Tuesday, March 3, 2026",
        "r1",
    )]);
    let imported = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    let pending = store.pending(Platform::Freshdirect).await.expect("pending");
    let message = &pending[0];
    let ParseAttempt::Parsed { parser, receipt } = Registry::default().parse(FIXTURE) else {
        panic!("the synthetic receipt parses");
    };
    let no_such_alias = AliasId::new(999_999).expect("a positive id");
    let finish = ParseFinish {
        message: message.id,
        expected_state: ParseState::Fetched,
        expected_version: message.version,
        next_state: ParseState::Parsed,
        attempt: AttemptRecord {
            parser,
            outcome: ParseOutcome::Ok,
            error: None,
            anomaly_count: 0,
        },
        order: Some(OrderWrite {
            receipt: &receipt,
            reconciled: true,
            sender_check: SenderCheck::Verified,
            alias_ids: vec![no_such_alias; receipt.lines.len()],
        }),
    };
    let result = store.finish_parse(&finish).await;
    assert!(
        matches!(
            result,
            Err(StoreError::Database {
                operation: "insert order line",
                ..
            })
        ),
        "{result:?}"
    );
    // The two writes before the one that failed are gone, and the message is as it was.
    assert_eq!(count(&pool, "select count(*) from parse_attempts").await, 0);
    assert_eq!(count(&pool, "select count(*) from grocery_orders").await, 0);
    assert_eq!(
        count(&pool, "select count(*) from grocery_order_lines").await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_messages where parse_state = 'fetched' and version = 0"
        )
        .await,
        1
    );
    // The control: the same message, still at the version that was read, is parsed by the
    // use case, which makes the aliases first. The failed write left nothing in its way.
    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!((parsed.pending, parsed.parsed), (1, 1));
    assert_eq!(count(&pool, "select count(*) from grocery_orders").await, 1);
    assert_eq!(
        count(&pool, "select count(*) from grocery_order_lines").await,
        i64::try_from(receipt.lines.len()).expect("a count")
    );
}

#[sqlx::test(migrations = false)]
async fn a_restaurant_receipt_is_stored_under_its_platform_and_waits_for_a_parser(pool: PgPool) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let seamless = RawMail {
        bytes: b"From: Seamless <orders@eat.seamless.com>\r\nTo: pat.example@example.com\r\n\
            Subject: Thanks for your Example Noodle Bar order\r\n\
            Date: Fri, 6 Mar 2026 19:05:00 -0500\r\nMessage-ID: <s1@mail.example>\r\n\
            Authentication-Results: mx.google.com; dkim=pass header.i=@eat.seamless.com\r\n\
            Content-Type: text/html; charset=utf-8\r\n\r\n\
            <html><body><p>Deliver to 123 Imaginary Street Apt 4B</p>\
            <p>1 Dan Dan Noodles $14.50</p></body></html>"
            .to_vec(),
    };
    let mails = Mails(vec![
        receipt_mail("12345678901", "Tuesday, March 3, 2026", "r1"),
        seamless,
    ]);

    let report = ImportMail::new(mails, reader(), store.clone())
        .run()
        .await
        .expect("import");
    assert_eq!(report.inserted, 2);
    assert_eq!(
        report.inserted_by_platform,
        std::collections::BTreeMap::from([(Platform::Freshdirect, 1), (Platform::Seamless, 1)])
    );
    let parsed = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &report.pages)
        .await
        .expect("parse");
    assert_eq!(parsed.pending, 1, "the parse step reads FreshDirect only");

    let waiting = store.pending(Platform::Seamless).await.expect("pending");
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].parse_state, ParseState::Fetched);
    // The record of the mail is there. Its page is not: no parser reads this platform.
    assert_eq!(waiting[0].body_html, None);

    let unknown = sqlx::query(
        "insert into raw_messages \
           (platform, message_key, key_kind, received_at, subject, kind, dkim_pass, source_sha256, redaction_version) \
         values ('ubereats', 'k9', 'message_id', now(), 'subject', 'receipt', true, '\\x09', 1)",
    )
    .execute(&pool)
    .await;
    let message = unknown.expect_err("the insert must fail").to_string();
    assert!(message.contains("raw_messages_platform_check"), "{message}");
}

#[sqlx::test(migrations = false)]
async fn a_receipt_without_a_dkim_pass_is_parsed_and_its_order_is_tagged_non_verified(
    pool: PgPool,
) {
    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let signed = receipt_mail("12345678901", "Tuesday, March 3, 2026", "v1");
    let unsigned = RawMail {
        bytes: String::from_utf8(
            receipt_mail("12345678902", "Tuesday, March 10, 2026", "v2").bytes,
        )
        .expect("utf8")
        .replace(
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n",
            "",
        )
        .into_bytes(),
    };
    let imported = ImportMail::new(Mails(vec![signed, unsigned]), reader(), store.clone())
        .run()
        .await
        .expect("import");
    let parsed = ParsePending::new(store, Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");
    assert_eq!((parsed.parsed, parsed.failed), (2, 0), "{parsed:?}");

    let tags: Vec<(String, String)> =
        sqlx::query_as("select order_number, sender_check from grocery_orders order by 1")
            .fetch_all(&pool)
            .await
            .expect("orders");
    assert_eq!(
        tags,
        [
            ("12345678901".to_owned(), "verified".to_owned()),
            ("12345678902".to_owned(), "non-verified".to_owned()),
        ]
    );
}

#[sqlx::test(migrations = false)]
async fn an_addition_keeps_its_reason_and_is_listed_until_a_later_order(pool: PgPool) {
    use qrtrmastr::application::list::ManageList;
    use qrtrmastr::application::recommend::HabitOptions;
    use qrtrmastr::domain::list::ItemText;
    use qrtrmastr::domain::states::AddReason;

    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let mails = Mails(vec![receipt_mail(
        "12345678901",
        "Tuesday, March 3, 2026",
        "r1",
    )]);
    let imported = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect, &imported.pages)
        .await
        .expect("parse");

    let list = ManageList::new(store.clone());
    let day = |d| jiff::civil::date(2026, 3, d);
    let text = |t: &str| ItemText::parse(t).expect("text");
    // Added before the order of March 3 was delivered: that order closed it.
    list.add(text("old thing"), AddReason::OutOfStock, day(1))
        .await
        .expect("add");
    let miso = list
        .add(text("miso  paste"), AddReason::NewFood, day(5))
        .await
        .expect("add");
    assert_eq!(miso.product, None);
    let milk = list
        .add(text("lactaid"), AddReason::Recipe, day(6))
        .await
        .expect("add");
    assert!(
        milk.product
            .as_ref()
            .is_some_and(|p| p.display_name.starts_with("Lactaid")),
        "{milk:?}"
    );

    let due = Due::new(store, Thresholds::default());
    let options = HabitOptions {
        extras: 10,
        remember: 5,
        seed: 1,
    };
    let report = due.run_habit(options, day(7)).await.expect("due");
    let listed: Vec<(&str, AddReason)> = report
        .additions
        .iter()
        .map(|a| (a.item.as_str(), a.reason))
        .collect();
    assert_eq!(
        listed,
        [
            ("miso paste", AddReason::NewFood),
            ("lactaid", AddReason::Recipe)
        ]
    );
    assert_eq!(report.additions[1].product, milk.product.map(|p| p.id));
    // One order: every product of it is in a quarter of the latest orders.
    assert!(!report.essentials.is_empty());
    assert!(report.extras.is_empty() && report.remember.is_empty());

    assert!(list.drop(miso.id).await.expect("drop"));
    assert!(!list.drop(miso.id).await.expect("drop again"));

    // Every reason of the enum is one the database admits.
    for reason in AddReason::ALL {
        list.add(text("ketchup"), *reason, day(7))
            .await
            .expect("add");
    }
    // The database refuses a reason the enum does not have.
    let refused = sqlx::query(
        "insert into list_additions (item_text, reason, added_on) values ('x', 'whim', '2026-03-07')",
    )
    .execute(&pool)
    .await;
    assert!(refused.is_err());
}

#[sqlx::test(migrations = false)]
async fn a_text_with_the_characters_of_sql_is_stored_and_read_back_as_that_text(pool: PgPool) {
    use qrtrmastr::application::list::ManageList;
    use qrtrmastr::application::ports::ShoppingList;
    use qrtrmastr::domain::list::ItemText;
    use qrtrmastr::domain::states::AddReason;

    let store = PostgresStore::from_pool(pool.clone());
    Migrate::new(store.clone()).run().await.expect("migrations");
    let list = ManageList::new(store.clone());
    let day = jiff::civil::date(2026, 3, 5);

    // A quote that would end a string, a statement of its own, a comment, an identifier
    // quote, a backslash, the wildcards of LIKE and a parameter marker.
    let hostile = r#"x'); drop table list_additions; -- "q" \ %_ $1"#;
    let plain = "miso paste";
    for item in [hostile, plain] {
        list.add(
            ItemText::parse(item).expect("text"),
            AddReason::NewFood,
            day,
        )
        .await
        .expect("add");
    }

    // The table is there, and it holds the two texts as they were given: SQL is a sink where
    // a value goes as a parameter, never as a piece of the statement (REAS-SEC-006).
    let stored: Vec<(String,)> = sqlx::query_as("select item_text from list_additions order by id")
        .fetch_all(&pool)
        .await
        .expect("the table exists");
    let stored: Vec<&str> = stored.iter().map(|(text,)| text.as_str()).collect();
    assert_eq!(stored, [hostile, plain]);

    let open = store.open_additions().await.expect("open additions");
    let read_back: Vec<&str> = open.iter().map(|a| a.item.as_str()).collect();
    assert_eq!(read_back, [hostile, plain]);
}
