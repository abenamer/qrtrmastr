//! `qm import`, second half: run the parsers over every stored message that waits in state
//! `fetched`, and write what they found.

use super::count;
use super::ports::{
    AliasRequest, AttemptRecord, Catalog, GroceryStore, OrderWrite, ParseAttempt, ParseFinish,
    ParserTag, PendingMessage, RawMessageStore, ReceiptParsing, StoreError,
};
use crate::domain::identity::normalize;
use crate::domain::receipt::{Anomaly, Receipt, reconcile};
use crate::domain::states::{MessageKind, ParseOutcome, ParseState, Platform, SenderCheck};

/// The parser tag of an attempt that no parser took: the message has no stored HTML body, or
/// no parser recognized the layout.
const REFUSED: ParserTag = ParserTag {
    name: "refused",
    version: 1,
};

/// What one parse run did, as counts of messages. After a run that ends without an error,
/// `pending` is the sum of `left_for_later`, `parsed`, `partial`, `failed` and `unsupported`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseReport {
    /// How many messages of the platform were in state `fetched` when the run started.
    pub pending: usize,
    /// How many pending messages are of kind `placed`, `updated` or `unknown`. No parser
    /// reads these kinds yet; the messages stay in `fetched`.
    pub left_for_later: usize,
    /// How many receipts gave an order that reconciles: outcome `ok`.
    pub parsed: usize,
    /// How many receipts gave an order that does not reconcile: outcome `partial`.
    pub partial: usize,
    /// How many receipts have no stored HTML body, or have a layout that a parser recognized
    /// and could not read: outcome `failed`.
    pub failed: usize,
    /// How many receipts have a layout that no parser recognizes: outcome `unsupported`.
    pub unsupported: usize,
    /// How many attempts stored no order and no attempt record because the state or the
    /// version of the message changed after it was read. A product or an alias that such an
    /// attempt made before that write stays. The outcome of such an attempt is counted before the
    /// write, so the message is also in one of the four counts above.
    pub conflicts: usize,
    /// The anomalies of all the receipts counted in `parsed` and `partial`: the rows that no
    /// parser rule classified and the mismatches of the reconciliation.
    pub anomalies: usize,
}

/// The second half of an import: for each stored receipt that waits, it runs the parsers,
/// reconciles the receipt, resolves each line to an alias, and writes the attempt with its
/// order in one transaction.
pub struct ParsePending<S, P>
where
    S: RawMessageStore + GroceryStore + Catalog,
    P: ReceiptParsing,
{
    store: S,
    parsers: P,
}

impl<S, P> ParsePending<S, P>
where
    S: RawMessageStore + GroceryStore + Catalog,
    P: ReceiptParsing,
{
    /// Builds the use case over the store and the receipt parsers.
    pub fn new(store: S, parsers: P) -> Self {
        Self { store, parsers }
    }

    /// Parses every message of `platform` in state `fetched`, oldest first. A conflict on one
    /// message is counted and the run goes on.
    ///
    /// # Errors
    ///
    /// The run stops at the first error. The attempts written before it stay written.
    ///
    /// - [`StoreError::Database`] when the connection fails, or the statement that lists the
    ///   pending messages, one that resolves an alias, or one that writes an attempt fails.
    /// - [`StoreError::InvalidRow`] when a pending message or the identity of an alias is not
    ///   a value that the domain accepts.
    ///
    /// A [`StoreError::Conflict`] is not returned; it is counted in the report.
    pub async fn run(&self, platform: Platform) -> Result<ParseReport, StoreError> {
        let mut report = ParseReport::default();
        let pending = self.store.pending(platform).await?;
        report.pending = pending.len();
        for message in &pending {
            match self.handle(message, &mut report).await {
                Ok(()) => {}
                Err(StoreError::Conflict { .. }) => count(&mut report.conflicts),
                Err(other) => return Err(other),
            }
        }
        Ok(report)
    }

    async fn handle(
        &self,
        message: &PendingMessage,
        report: &mut ParseReport,
    ) -> Result<(), StoreError> {
        match message.kind {
            MessageKind::Placed | MessageKind::Updated | MessageKind::Unknown => {
                // No parser for these kinds yet; they stay fetched for a later milestone.
                count(&mut report.left_for_later);
                return Ok(());
            }
            MessageKind::Receipt => {}
        }

        // A receipt without a DKIM pass is parsed all the same: the mailbox is the maintainer's
        // own, and the order is tagged non-verified instead (ADR 0008).
        let Some(html) = message.body_html.as_deref() else {
            count(&mut report.failed);
            return self
                .finish(
                    message,
                    REFUSED,
                    ParseOutcome::Failed,
                    Some("no HTML part".to_owned()),
                    None,
                )
                .await;
        };

        match self.parsers.parse(html) {
            ParseAttempt::Unsupported => {
                count(&mut report.unsupported);
                self.finish(
                    message,
                    REFUSED,
                    ParseOutcome::Unsupported,
                    Some("no parser recognizes this layout".to_owned()),
                    None,
                )
                .await
            }
            ParseAttempt::Failed { parser, error } => {
                count(&mut report.failed);
                self.finish(message, parser, ParseOutcome::Failed, Some(error), None)
                    .await
            }
            ParseAttempt::Parsed { parser, receipt } => {
                let mut receipt = *receipt;
                let reconciliation = reconcile(&receipt);
                for issue in &reconciliation.line_issues {
                    receipt.anomalies.push(Anomaly {
                        row_index: 0,
                        cells: Vec::new(),
                        reason: format!(
                            "line {} prints {} but unit price times billed quantity is {}",
                            issue.line_no, issue.printed, issue.expected
                        ),
                    });
                }
                if !reconciliation.subtotal_matches {
                    receipt.anomalies.push(Anomaly {
                        row_index: 0,
                        cells: Vec::new(),
                        reason: format!(
                            "lines sum to {} but the subtotal is {}",
                            reconciliation.line_sum, receipt.totals.subtotal
                        ),
                    });
                }
                if !reconciliation.total_matches {
                    receipt.anomalies.push(Anomaly {
                        row_index: 0,
                        cells: Vec::new(),
                        reason: "the subtotal and its additions do not sum to the total".to_owned(),
                    });
                }
                let reconciled = reconciliation.reconciled();
                let outcome = if reconciled {
                    count(&mut report.parsed);
                    ParseOutcome::Ok
                } else {
                    count(&mut report.partial);
                    ParseOutcome::Partial
                };
                report.anomalies = report.anomalies.saturating_add(receipt.anomalies.len());

                let alias_ids = self.resolve_aliases(&receipt).await?;
                let order = OrderWrite {
                    receipt: &receipt,
                    reconciled,
                    sender_check: SenderCheck::from_dkim_pass(message.dkim_pass),
                    alias_ids,
                };
                self.finish(message, parser, outcome, None, Some(order))
                    .await
            }
        }
    }

    async fn resolve_aliases(
        &self,
        receipt: &Receipt,
    ) -> Result<Vec<super::ports::AliasId>, StoreError> {
        let mut ids = Vec::with_capacity(receipt.lines.len());
        for line in &receipt.lines {
            let normalized = normalize(&line.raw_name);
            let request = AliasRequest {
                normalized: &normalized,
                raw_name: &line.raw_name,
                sold_by: line.sold_by(),
                department: line.department.as_deref(),
                product_url_path: line.product_url_path.as_deref(),
            };
            ids.push(self.store.resolve_alias(&request).await?);
        }
        Ok(ids)
    }

    async fn finish(
        &self,
        message: &PendingMessage,
        parser: ParserTag,
        outcome: ParseOutcome,
        error: Option<String>,
        order: Option<OrderWrite<'_>>,
    ) -> Result<(), StoreError> {
        let next_state = message
            .parse_state
            .first_parse(outcome)
            .or_else(|_| message.parse_state.reparse(outcome))
            .map_err(|transition| StoreError::InvalidRow {
                table: "raw_messages",
                id: message.id.get(),
                reason: transition.to_string(),
            })?;
        let anomaly_count = order.as_ref().map_or(0, |o| {
            i32::try_from(o.receipt.anomalies.len()).unwrap_or(i32::MAX)
        });
        let finish = ParseFinish {
            message: message.id,
            expected_state: message.parse_state,
            expected_version: message.version,
            next_state,
            attempt: AttemptRecord {
                parser,
                outcome,
                error,
                anomaly_count,
            },
            order,
        };
        debug_assert!(matches!(
            finish.next_state,
            ParseState::Parsed | ParseState::Partial | ParseState::Failed | ParseState::Unsupported
        ));
        self.store.finish_parse(&finish).await
    }
}
