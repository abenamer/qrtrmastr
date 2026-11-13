//! The command line, an inbound adapter: it turns arguments into use-case calls in `main.rs`
//! and renders the reports the use cases return.

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Parser, Subcommand};

use crate::application::import::ImportReport;
use crate::application::parse::ParseReport;
use crate::application::recommend::{DueReport, SimpleReport};
use crate::domain::recommend::Status;

#[derive(Debug, Parser)]
#[command(
    name = "qm",
    version,
    about = "Quartermaster: a grocery order history from receipt emails, and a recommended next cart"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply the pending database migrations. Running it again applies nothing.
    Migrate,
    /// Import a Google Takeout export of your FreshDirect mail, then parse the new receipts.
    /// Idempotent: messages already stored are skipped.
    Import {
        /// Path to the Takeout export of your FreshDirect label.
        #[arg(long, value_name = "PATH")]
        mbox: PathBuf,
    },
    /// What you are about to run out of, from your own purchase gaps.
    Due {
        /// The day to judge from; today when omitted.
        #[arg(long, value_name = "YYYY-MM-DD")]
        as_of: Option<NaiveDate>,
        /// Also list items bought only twice.
        #[arg(long)]
        include_tentative: bool,
        /// Also list items not bought for more than three usual gaps.
        #[arg(long)]
        include_lapsed: bool,
        /// Also list items that are not due yet.
        #[arg(long)]
        include_not_due: bool,
        /// Use the simple rule instead: in at least half your orders and missing from the last.
        #[arg(long)]
        simple: bool,
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Work with parser fixtures.
    Fixture {
        #[command(subcommand)]
        action: FixtureAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum FixtureAction {
    /// Write the stored, redacted HTML of one message to the local fixtures directory, which
    /// git ignores. The key is the message key `qm import` stored.
    Add {
        key: String,
        /// Where to write; the default is the gitignored local directory.
        #[arg(long, default_value = "tests/fixtures/local")]
        dir: PathBuf,
    },
}

pub fn render_import(import: &ImportReport, parse: &ParseReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "messages read {}, new {}, already known {}, from other senders {}\n",
        import.read, import.inserted, import.already_known, import.other_sender
    ));
    for m in &import.malformed {
        out.push_str(&format!("malformed: {m}\n"));
    }
    for r in &import.redaction_failures {
        out.push_str(&format!("REFUSED, personal data survived redaction: {r}\n"));
    }
    out.push_str(&format!(
        "receipts parsed {}, partial {}, failed {}, unsupported {}, left for later {}, anomalies {}",
        parse.parsed, parse.partial, parse.failed, parse.unsupported, parse.left_for_later, parse.anomalies
    ));
    if parse.conflicts > 0 {
        out.push_str(&format!(", conflicts {}", parse.conflicts));
    }
    out.push('\n');
    out
}

fn label(row_product: &crate::application::ports::ProductLabel) -> String {
    match &row_product.size_text {
        Some(size) => format!("{} ({size})", row_product.display_name),
        None => row_product.display_name.clone(),
    }
}

fn status_text(status: Status) -> String {
    match status {
        Status::Overdue { days_overdue } => format!("overdue by {days_overdue}d"),
        Status::DueSoon { days_until_due } => format!("due in {days_until_due}d"),
        Status::Tentative => "tentative".to_owned(),
        Status::Lapsed => "lapsed".to_owned(),
        Status::NotDue => "not due".to_owned(),
        Status::BoughtOnce => "bought once".to_owned(),
    }
}

pub fn render_due(report: &DueReport) -> String {
    let mut out = format!(
        "as of {}, {} orders, usual gap between orders {}\n",
        report.as_of,
        report.orders,
        report
            .order_cadence_days
            .map_or("unknown".to_owned(), |d| format!("{d} days"))
    );
    if report.rows.is_empty() {
        out.push_str("nothing is due\n");
        return out;
    }
    out.push_str(&format!(
        "{:<48} {:>6} {:>8} {:>11} {:>6} {:>5}  {}\n",
        "product", "bought", "gap", "last", "since", "qty", "status"
    ));
    for row in &report.rows {
        let r = &row.recommendation;
        out.push_str(&format!(
            "{:<48} {:>6} {:>8} {:>11} {:>6} {:>5}  {}\n",
            truncate(&label(&row.product), 48),
            r.times_bought,
            r.usual_gap_days.map_or("-".to_owned(), |g| format!("{g}d")),
            r.last_bought,
            format!("{}d", r.days_since),
            r.typical_quantity,
            status_text(r.status)
        ));
    }
    out
}

pub fn render_simple(report: &SimpleReport) -> String {
    let mut out = format!(
        "{} orders; items in at least half of them and missing from the last:\n",
        report.orders
    );
    if report.rows.is_empty() {
        out.push_str("none\n");
        return out;
    }
    out.push_str(&format!("{:<48} {:>8} {:>5}\n", "product", "orders", "qty"));
    for row in &report.rows {
        let r = &row.recommendation;
        out.push_str(&format!(
            "{:<48} {:>8} {:>5}\n",
            truncate(&label(&row.product), 48),
            format!("{}/{}", r.orders_with_product, r.orders_total),
            r.typical_quantity
        ));
    }
    out
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(width - 1).collect();
        format!("{cut}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_parse() {
        let cli = Cli::try_parse_from(["qm", "migrate"]).expect("valid arguments");
        assert!(matches!(cli.command, Command::Migrate));
        let cli = Cli::try_parse_from(["qm", "import", "--mbox", "/tmp/export"]).expect("valid");
        assert!(matches!(cli.command, Command::Import { mbox } if mbox == *"/tmp/export"));
        let cli = Cli::try_parse_from(["qm", "fixture", "add", "abc@mail.example"]).expect("valid");
        assert!(matches!(
            cli.command,
            Command::Fixture {
                action: FixtureAction::Add { .. }
            }
        ));
        let cli = Cli::try_parse_from(["qm", "due", "--as-of", "2026-03-03", "--simple", "--json"])
            .expect("valid");
        assert!(matches!(
            cli.command,
            Command::Due {
                simple: true,
                json: true,
                as_of: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn an_unknown_command_and_a_bad_date_are_errors() {
        assert!(Cli::try_parse_from(["qm", "launch"]).is_err());
        assert!(Cli::try_parse_from(["qm", "due", "--as-of", "yesterday"]).is_err());
        assert!(Cli::try_parse_from(["qm", "import"]).is_err());
    }
}
