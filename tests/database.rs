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
    assert_eq!(first.applied.len(), 2, "{:?}", first.applied);
    assert!(
        first.applied[0].ends_with("raw mail"),
        "{:?}",
        first.applied
    );

    let second = migrate.run().await.expect("second migration run");
    assert!(second.applied.is_empty(), "{:?}", second.applied);
    assert_eq!(second.already_applied, 2);
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

use chrono::NaiveDate;
use qrtrmastr::adapters::mail::FreshDirectMail;
use qrtrmastr::adapters::mail::redact::{RedactionTerms, Redactor};
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::import::ImportMail;
use qrtrmastr::application::parse::ParsePending;
use qrtrmastr::application::ports::{
    AttemptRecord, GroceryStore, MailError, MailSource, ParseFinish, ParserTag, RawMail,
    RawMessageStore, StoreError,
};
use qrtrmastr::application::recommend::{Due, DueOptions};
use qrtrmastr::domain::recommend::{Status, Thresholds};
use qrtrmastr::domain::states::{ParseOutcome, ParseState, Platform};

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

#[derive(Clone)]
struct Mails(Vec<RawMail>);

impl MailSource for Mails {
    async fn messages(&self) -> Result<Vec<RawMail>, MailError> {
        Ok(self.0.clone())
    }
}

fn reader() -> FreshDirectMail {
    FreshDirectMail::new(
        vec!["service@freshdirect.com".to_owned()],
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
    let as_of = NaiveDate::from_ymd_opt(2026, 3, 27).expect("date");
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
        expected_version: message.version + 1,
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
