//! `PostgresStore`: the PostgreSQL adapter behind every store port (REAS-DATA-001).
//!
//! Each port operation is one specific SQL statement in this module. Writes that must succeed
//! or fail together run inside one transaction that this adapter owns (REAS-DATA-002).

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::application::ports::{MigrationReport, SchemaStore, StoreError};

/// The migrations under `migrations/`, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
            .map_err(|source| database_error("connect", source))?;
        Ok(Self { pool })
    }

    /// Wraps a pool a test harness created.
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn database_error(operation: &'static str, source: sqlx::Error) -> StoreError {
    StoreError::Database {
        operation,
        source: Box::new(source),
    }
}

/// The versions SQLx has recorded as applied, or none when the table does not exist yet.
async fn applied_migrations(pool: &PgPool) -> Result<Vec<(i64, String)>, StoreError> {
    let (table,): (Option<String>,) =
        sqlx::query_as("select to_regclass('_sqlx_migrations')::text")
            .fetch_one(pool)
            .await
            .map_err(|source| database_error("check the migrations table", source))?;
    if table.is_none() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, (i64, String)>(
        "select version, description from _sqlx_migrations where success order by version",
    )
    .fetch_all(pool)
    .await
    .map_err(|source| database_error("list applied migrations", source))
}

impl SchemaStore for PostgresStore {
    async fn migrate(&self) -> Result<MigrationReport, StoreError> {
        let before = applied_migrations(&self.pool).await?;
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        let after = applied_migrations(&self.pool).await?;
        let applied = after
            .iter()
            .filter(|(version, _)| !before.iter().any(|(known, _)| known == version))
            .map(|(version, description)| format!("{version} {description}"))
            .collect();
        Ok(MigrationReport {
            applied,
            already_applied: before.len(),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Raw messages, orders and the catalog.
// ---------------------------------------------------------------------------------------------

use std::collections::HashSet;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::application::ports::{
    AliasRequest, Catalog, GroceryStore, InsertOutcome, NewRawMessage, ParseFinish, PendingMessage,
    ProductLabel, RawMessageStore, StoredBody,
};
use crate::domain::ids::{AliasId, MessageKey, OrderNumber, ProductId, RawMessageId};
use crate::domain::money::{Money, Quantity};
use crate::domain::recommend::Purchase;
use crate::domain::states::{KeyKind, MessageKind, ParseState, Platform};

fn invalid_row(table: &'static str, id: i64, reason: impl ToString) -> StoreError {
    StoreError::InvalidRow {
        table,
        id,
        reason: reason.to_string(),
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    id: i64,
    message_key: String,
    key_kind: String,
    kind: String,
    dkim_pass: bool,
    received_at: DateTime<Utc>,
    body_html: Option<String>,
    parse_state: String,
    version: i32,
}

impl PendingRow {
    fn into_domain(self) -> Result<PendingMessage, StoreError> {
        let id = self.id;
        let key_kind =
            KeyKind::from_str(&self.key_kind).map_err(|e| invalid_row("raw_messages", id, e))?;
        let key = match key_kind {
            KeyKind::MessageId => MessageKey::MessageId(self.message_key),
            KeyKind::Sha256 => MessageKey::Sha256(self.message_key),
        };
        Ok(PendingMessage {
            id: RawMessageId::new(id).map_err(|e| invalid_row("raw_messages", id, e))?,
            key,
            kind: MessageKind::from_str(&self.kind)
                .map_err(|e| invalid_row("raw_messages", id, e))?,
            dkim_pass: self.dkim_pass,
            received_at: self.received_at,
            body_html: self.body_html,
            parse_state: ParseState::from_str(&self.parse_state)
                .map_err(|e| invalid_row("raw_messages", id, e))?,
            version: self.version,
        })
    }
}

#[derive(sqlx::FromRow)]
struct BodyRow {
    id: i64,
    platform: String,
    message_key: String,
    key_kind: String,
    received_at: DateTime<Utc>,
    body_html: Option<String>,
}

impl RawMessageStore for PostgresStore {
    async fn body_by_key(&self, key: &str) -> Result<Option<StoredBody>, StoreError> {
        let row: Option<BodyRow> = sqlx::query_as(
            "select id, platform, message_key, key_kind, received_at, body_html from raw_messages where message_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database_error("find message by key", e))?;
        row.map(|row| {
            let id = row.id;
            let key = match KeyKind::from_str(&row.key_kind)
                .map_err(|e| invalid_row("raw_messages", id, e))?
            {
                KeyKind::MessageId => MessageKey::MessageId(row.message_key),
                KeyKind::Sha256 => MessageKey::Sha256(row.message_key),
            };
            Ok(StoredBody {
                platform: Platform::from_str(&row.platform)
                    .map_err(|e| invalid_row("raw_messages", id, e))?,
                key,
                received_at: row.received_at,
                body_html: row.body_html,
            })
        })
        .transpose()
    }

    async fn known_keys(&self) -> Result<HashSet<String>, StoreError> {
        let rows: Vec<(String,)> = sqlx::query_as("select message_key from raw_messages")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database_error("list message keys", e))?;
        Ok(rows.into_iter().map(|(key,)| key).collect())
    }

    async fn insert(&self, message: &NewRawMessage) -> Result<InsertOutcome, StoreError> {
        let row: Option<(i64,)> = sqlx::query_as(
            "insert into raw_messages \
               (platform, message_key, key_kind, received_at, subject, kind, dkim_pass, \
                body_html, body_text, source_sha256, redaction_version) \
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             on conflict do nothing \
             returning id",
        )
        .bind(message.platform.as_str())
        .bind(message.key.as_str())
        .bind(message.key.kind().as_str())
        .bind(message.received_at)
        .bind(&message.subject)
        .bind(message.kind.as_str())
        .bind(message.dkim_pass)
        .bind(&message.body_html)
        .bind(&message.body_text)
        .bind(&message.source_sha256[..])
        .bind(message.redaction_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database_error("insert raw message", e))?;
        match row {
            Some((id,)) => Ok(InsertOutcome::Inserted(
                RawMessageId::new(id).map_err(|e| invalid_row("raw_messages", id, e))?,
            )),
            None => Ok(InsertOutcome::AlreadyKnown),
        }
    }

    async fn pending(&self, platform: Platform) -> Result<Vec<PendingMessage>, StoreError> {
        let rows: Vec<PendingRow> = sqlx::query_as(
            "select id, message_key, key_kind, kind, dkim_pass, received_at, body_html, parse_state, version \
             from raw_messages where platform = $1 and parse_state = 'fetched' \
             order by received_at, id",
        )
        .bind(platform.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| database_error("list pending messages", e))?;
        rows.into_iter().map(PendingRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct PurchaseRow {
    product_id: i64,
    order_number: String,
    delivery_date: NaiveDate,
    billed_qty: Decimal,
    final_price: Decimal,
}

impl GroceryStore for PostgresStore {
    async fn finish_parse(&self, finish: &ParseFinish<'_>) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database_error("begin", e))?;
        let message_id = finish.message.get();
        sqlx::query(
            "insert into parse_attempts (raw_message_id, parser_name, parser_version, outcome, error, anomaly_count) \
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(message_id)
        .bind(finish.attempt.parser.name)
        .bind(finish.attempt.parser.version)
        .bind(finish.attempt.outcome.as_str())
        .bind(&finish.attempt.error)
        .bind(finish.attempt.anomaly_count)
        .execute(&mut *tx)
        .await
        .map_err(|e| database_error("insert parse attempt", e))?;

        let mut order_number: Option<&str> = None;
        if let Some(order) = &finish.order {
            let receipt = order.receipt;
            let number = receipt.order_number.as_str();
            order_number = Some(number);
            sqlx::query(
                "delete from order_line_anomalies where order_number = $1 or raw_message_id = $2",
            )
            .bind(number)
            .bind(message_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| database_error("delete anomalies", e))?;
            sqlx::query("delete from grocery_orders where order_number = $1")
                .bind(number)
                .execute(&mut *tx)
                .await
                .map_err(|e| database_error("delete order", e))?;
            sqlx::query(
                "insert into grocery_orders \
                   (order_number, delivery_date, delivery_window, receipt_raw_message_id, parser_name, parser_version, \
                    subtotal, tax, bottle_deposit, delivery_fee, service_fee, tip, total, reconciled) \
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            )
            .bind(number)
            .bind(receipt.delivery_date)
            .bind(&receipt.delivery_window)
            .bind(message_id)
            .bind(finish.attempt.parser.name)
            .bind(finish.attempt.parser.version)
            .bind(receipt.totals.subtotal.as_decimal())
            .bind(receipt.totals.tax.map(Money::as_decimal))
            .bind(receipt.totals.bottle_deposit.map(Money::as_decimal))
            .bind(receipt.totals.delivery_fee.map(Money::as_decimal))
            .bind(receipt.totals.service_fee.map(Money::as_decimal))
            .bind(receipt.totals.tip.map(Money::as_decimal))
            .bind(receipt.totals.total.as_decimal())
            .bind(order.reconciled)
            .execute(&mut *tx)
            .await
            .map_err(|e| database_error("insert order", e))?;

            for (line, alias) in receipt.lines.iter().zip(&order.alias_ids) {
                let flags: Vec<String> = line
                    .flags
                    .as_letters()
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                sqlx::query(
                    "insert into grocery_order_lines \
                       (order_number, line_no, department, raw_name, alias_id, qty_first, qty_second, weight, weight_unit, \
                        unit_price, unit_price_per, options_price, final_price, flags, billed_qty) \
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
                )
                .bind(number)
                .bind(i32::try_from(line.line_no).unwrap_or(i32::MAX))
                .bind(&line.department)
                .bind(&line.raw_name)
                .bind(alias.get())
                .bind(line.qty_first.as_decimal())
                .bind(line.qty_second.as_decimal())
                .bind(line.weight.as_ref().map(|w| w.value.as_decimal()))
                .bind(line.weight.as_ref().map(|w| w.unit.as_str()))
                .bind(line.unit_price.map(|u| u.amount.as_decimal()))
                .bind(line.unit_price.map(|u| u.per.as_str()))
                .bind(line.options_price.map(Money::as_decimal))
                .bind(line.final_price.as_decimal())
                .bind(&flags)
                .bind(line.billed_qty().as_decimal())
                .execute(&mut *tx)
                .await
                .map_err(|e| database_error("insert order line", e))?;
            }
            for anomaly in &receipt.anomalies {
                sqlx::query(
                    "insert into order_line_anomalies (raw_message_id, order_number, row_index, raw_cells, reason) \
                     values ($1, $2, $3, $4, $5)",
                )
                .bind(message_id)
                .bind(number)
                .bind(i32::try_from(anomaly.row_index).unwrap_or(i32::MAX))
                .bind(serde_json::Value::from(anomaly.cells.clone()))
                .bind(&anomaly.reason)
                .execute(&mut *tx)
                .await
                .map_err(|e| database_error("insert anomaly", e))?;
            }
        }

        // The guarded transition (REAS-STATE-003): exactly one row, or nothing happened.
        let result = sqlx::query(
            "update raw_messages \
             set parse_state = $1, version = version + 1, order_number = coalesce($2, order_number) \
             where id = $3 and parse_state = $4 and version = $5",
        )
        .bind(finish.next_state.as_str())
        .bind(order_number)
        .bind(message_id)
        .bind(finish.expected_state.as_str())
        .bind(finish.expected_version)
        .execute(&mut *tx)
        .await
        .map_err(|e| database_error("transition message", e))?;
        if result.rows_affected() != 1 {
            // Dropping the transaction rolls it back.
            return Err(StoreError::Conflict {
                message: finish.message,
                expected: finish.expected_state,
                version: finish.expected_version,
            });
        }
        tx.commit().await.map_err(|e| database_error("commit", e))
    }

    async fn purchases(&self) -> Result<Vec<Purchase>, StoreError> {
        let rows: Vec<PurchaseRow> = sqlx::query_as(
            "select product_id, order_number, delivery_date, billed_qty, final_price from purchases",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| database_error("list purchases", e))?;
        rows.into_iter()
            .map(|row| {
                let id = row.product_id;
                Ok(Purchase {
                    product: ProductId::new(id).map_err(|e| invalid_row("purchases", id, e))?,
                    order_number: OrderNumber::parse(&row.order_number)
                        .map_err(|e| invalid_row("purchases", id, e))?,
                    delivery_date: row.delivery_date,
                    quantity: Quantity::new(row.billed_qty)
                        .map_err(|e| invalid_row("purchases", id, e))?,
                    price: Money::from_decimal(row.final_price)
                        .map_err(|e| invalid_row("purchases", id, e))?,
                })
            })
            .collect()
    }

    async fn delivery_dates(&self) -> Result<Vec<NaiveDate>, StoreError> {
        let rows: Vec<(NaiveDate,)> =
            sqlx::query_as("select delivery_date from grocery_orders order by delivery_date")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| database_error("list delivery dates", e))?;
        Ok(rows.into_iter().map(|(d,)| d).collect())
    }

    async fn muted_products(&self) -> Result<HashSet<ProductId>, StoreError> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("select product_id from product_preferences where muted")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| database_error("list muted products", e))?;
        rows.into_iter()
            .map(|(id,)| ProductId::new(id).map_err(|e| invalid_row("product_preferences", id, e)))
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct LabelRow {
    id: i64,
    display_name: String,
    size_text: Option<String>,
    department: Option<String>,
    latest_raw_name: Option<String>,
}

impl Catalog for PostgresStore {
    async fn resolve_alias(&self, request: &AliasRequest<'_>) -> Result<AliasId, StoreError> {
        let name_norm = request.normalized.name_norm.as_str();
        let size_norm = request.normalized.size_norm.as_str();
        let existing: Option<(i64,)> = sqlx::query_as(
            "select id from product_aliases where name_norm = $1 and size_norm = $2",
        )
        .bind(name_norm)
        .bind(size_norm)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database_error("find alias", e))?;
        if let Some((id,)) = existing {
            return AliasId::new(id).map_err(|e| invalid_row("product_aliases", id, e));
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database_error("begin", e))?;
        let (product_id,): (i64,) = sqlx::query_as(
            "insert into products (display_name, size_text, sold_by, department) values ($1, $2, $3, $4) returning id",
        )
        .bind(&request.normalized.display_name)
        .bind(&request.normalized.size_text)
        .bind(request.sold_by.as_str())
        .bind(request.department)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| database_error("insert product", e))?;
        let inserted: Option<(i64,)> = sqlx::query_as(
            "insert into product_aliases (name_norm, size_norm, product_url_path, product_id, source, first_seen_raw_name) \
             values ($1, $2, $3, $4, 'auto', $5) on conflict (name_norm, size_norm) do nothing returning id",
        )
        .bind(name_norm)
        .bind(size_norm)
        .bind(request.product_url_path)
        .bind(product_id)
        .bind(request.raw_name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| database_error("insert alias", e))?;
        match inserted {
            Some((id,)) => {
                tx.commit().await.map_err(|e| database_error("commit", e))?;
                AliasId::new(id).map_err(|e| invalid_row("product_aliases", id, e))
            }
            None => {
                // Someone else inserted the same alias meanwhile: keep theirs, drop our product.
                drop(tx);
                let (id,): (i64,) = sqlx::query_as(
                    "select id from product_aliases where name_norm = $1 and size_norm = $2",
                )
                .bind(name_norm)
                .bind(size_norm)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| database_error("find alias again", e))?;
                AliasId::new(id).map_err(|e| invalid_row("product_aliases", id, e))
            }
        }
    }

    async fn product_labels(&self) -> Result<Vec<ProductLabel>, StoreError> {
        let rows: Vec<LabelRow> = sqlx::query_as(
            "select p.id, p.display_name, p.size_text, p.department, \
                    (select l.raw_name from grocery_order_lines l \
                       join product_aliases a on a.id = l.alias_id \
                       join grocery_orders o on o.order_number = l.order_number \
                      where a.product_id = p.id order by o.delivery_date desc, l.line_no limit 1) as latest_raw_name \
             from products p order by p.id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| database_error("list product labels", e))?;
        rows.into_iter()
            .map(|row| {
                let id = row.id;
                Ok(ProductLabel {
                    id: ProductId::new(id).map_err(|e| invalid_row("products", id, e))?,
                    latest_raw_name: row
                        .latest_raw_name
                        .unwrap_or_else(|| row.display_name.clone()),
                    display_name: row.display_name,
                    size_text: row.size_text,
                    department: row.department,
                })
            })
            .collect()
    }
}
