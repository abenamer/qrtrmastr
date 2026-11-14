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
