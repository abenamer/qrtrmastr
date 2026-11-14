//! `qm import`, second half: run the parsers over every stored message that waits in state
//! `fetched`, and write what they found.

use serde::Serialize;

use super::ports::{
    AliasRequest, AttemptRecord, Catalog, GroceryStore, OrderWrite, ParseAttempt, ParseFinish,
    ParserTag, PendingMessage, RawMessageStore, ReceiptParsing, StoreError,
};
use crate::domain::identity::normalize;
use crate::domain::receipt::{Anomaly, Receipt, reconcile};
use crate::domain::states::{MessageKind, ParseOutcome, ParseState, Platform};

/// The parser tag recorded when no parser ran at all.
const REFUSED: ParserTag = ParserTag {
    name: "refused",
    version: 1,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParseReport {
    pub pending: usize,
    pub left_for_later: usize,
    pub parsed: usize,
    pub partial: usize,
    pub failed: usize,
    pub unsupported: usize,
    pub conflicts: usize,
    pub anomalies: usize,
}

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
    pub fn new(store: S, parsers: P) -> Self {
        Self { store, parsers }
    }

    pub async fn run(&self, platform: Platform) -> Result<ParseReport, StoreError> {
        let mut report = ParseReport::default();
        let pending = self.store.pending(platform).await?;
        report.pending = pending.len();
        for message in &pending {
            match self.handle(message, &mut report).await {
                Ok(()) => {}
                Err(StoreError::Conflict { .. }) => report.conflicts += 1,
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
                report.left_for_later += 1;
                return Ok(());
            }
            MessageKind::Receipt => {}
        }

        if !message.dkim_pass {
            report.failed += 1;
            return self
                .finish(
                    message,
                    REFUSED,
                    ParseOutcome::Failed,
                    Some("DKIM did not pass; the message is not trusted".to_owned()),
                    None,
                )
                .await;
        }
        let Some(html) = message.body_html.as_deref() else {
            report.failed += 1;
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
                report.unsupported += 1;
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
                report.failed += 1;
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
                    report.parsed += 1;
                    ParseOutcome::Ok
                } else {
                    report.partial += 1;
                    ParseOutcome::Partial
                };
                report.anomalies += receipt.anomalies.len();

                let alias_ids = self.resolve_aliases(&receipt).await?;
                let order = OrderWrite {
                    receipt: &receipt,
                    reconciled,
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
