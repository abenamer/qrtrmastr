//! Integration tests against a real PostgreSQL (REAS-TEST-001). `#[sqlx::test]` creates a fresh
//! database per test from `DATABASE_URL`; CI sets it, and `docs/runbooks/develop.md` says how to
//! set it locally. Migrations are applied through the store, not by the harness, so the store's
//! own path is what is tested.
#![expect(
    clippy::expect_used,
    reason = "test code, which REAS-RUST-005 excludes; the helpers below are outside a test function, where `clippy.toml` does not reach"
)]

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
use qrtrmastr::adapters::mail::redact::{RedactionTerms, Redactor};
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::import::ImportMail;
use qrtrmastr::application::parse::ParsePending;
use qrtrmastr::application::ports::{
    AttemptRecord, GroceryStore, InsertOutcome, MailSource, MailStream, NewRawMessage, ParseFinish,
    ParserTag, RawMail, RawMessageStore, StoreError,
};
use qrtrmastr::application::recommend::{Due, DueOptions};
use qrtrmastr::domain::ids::{IdError, MessageKey};
use qrtrmastr::domain::recommend::{Status, Thresholds};
use qrtrmastr::domain::states::{KeyKind, MessageKind, ParseOutcome, ParseState, Platform};
use std::collections::HashSet;

const FIXTURE: &str = include_str!("fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

fn terms() -> RedactionTerms {
    RedactionTerms {
        names: vec!["Pat Example".to_owned(), "Pat".to_owned()],
        emails: vec!["pat.example@example.com".to_owned()],
        phones: vec!["2125550199".to_owned()],
        address_lines: vec![
            "123 Imaginary Street Apt 4B".to_owned(),
            "Brooklyn, NY 11201".to_owned(),
        ],
        card_last4: vec!["4242".to_owned()],
    }
}

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

    let body = store
        .body_by_key(&from_header)
        .await
        .expect("the lookup")
        .expect("the imported message");
    assert_eq!(body.key, from_header);
    assert_eq!(body.platform, Platform::Freshdirect);
    assert!(
        body.body_html
            .is_some_and(|html| html.contains("12345678901"))
    );
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
            subject: "subject".to_owned(),
            kind: MessageKind::Receipt,
            dkim_pass: true,
            body_html: None,
            body_text: None,
            source_sha256: [7; 32],
            redaction_version: 1,
        })
        .await
        .expect("the insert");
    assert_eq!(outcome, InsertOutcome::AlreadyKnown);
    assert_eq!(count(&pool, "select count(*) from raw_messages").await, 2);
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
    PlatformMail::new(
        vec![
            ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
            ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
        ],
        Redactor::new(terms()).expect("terms"),
    )
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
        .run(Platform::Freshdirect)
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

    // Nothing personal reached the store: the configured terms are absent from every body.
    let leaks = count(
        &pool,
        "select count(*) from raw_messages where body_html ilike '%Imaginary Street%' \
           or body_html ilike '%555-0199%' or body_html ilike '%4242%' or body_html ilike '%pat.example@%'",
    )
    .await;
    assert_eq!(leaks, 0);

    let second = ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("second import");
    assert_eq!(
        (second.read, second.inserted, second.already_known),
        (3, 0, 3)
    );
    let parsed_again = ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect)
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
    ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect)
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
        .run(Platform::Freshdirect)
        .await
        .expect("parse");
    assert_eq!(parsed.pending, 1, "the parse step reads FreshDirect only");

    let waiting = store.pending(Platform::Seamless).await.expect("pending");
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].parse_state, ParseState::Fetched);
    let html = waiting[0].body_html.as_deref().expect("html");
    assert!(html.contains("Dan Dan Noodles"), "{html}");
    assert!(!html.contains("Imaginary Street"), "{html}");

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
    ImportMail::new(Mails(vec![signed, unsigned]), reader(), store.clone())
        .run()
        .await
        .expect("import");
    let parsed = ParsePending::new(store, Registry::default())
        .run(Platform::Freshdirect)
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
    ImportMail::new(mails.clone(), reader(), store.clone())
        .run()
        .await
        .expect("import");
    ParsePending::new(store.clone(), Registry::default())
        .run(Platform::Freshdirect)
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
