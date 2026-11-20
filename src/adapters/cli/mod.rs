//! The command line, an inbound adapter: it turns arguments into use-case calls in `main.rs`
//! and renders the reports the use cases return.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use jiff::civil::Date;

use crate::application::import::ImportReport;
use crate::application::parse::ParseReport;
use crate::application::recommend::{DueReport, HabitReport, SimpleReport};
use crate::domain::recommend::Status;

#[derive(Debug, Parser)]
#[command(
    name = "qm",
    version,
    about = "Quartermaster: a grocery order history from receipt emails, and a recommended next cart",
    long_about = "Quartermaster: a grocery order history from receipt emails, and a recommended next cart.\n\n\
        The tool needs a Google Takeout export of your mail. It talks to no retailer \
        and reads no mailbox. In Gmail, give the receipts a label with a filter on \
        from:service@freshdirect.com; at https://takeout.google.com export Mail, that label only, \
        as a .zip; put the archive in your exports directory (~/qm-data, /data inside the container) \
        and import that folder: archives are read in place, one after the other. An export of \
        the whole mailbox works too: only mail from FreshDirect and from the restaurant \
        platforms in the configuration (Seamless, Grubhub, Toast, Square, Clover, DoorDash) is \
        stored, and only FreshDirect receipts are parsed so far. \
        Before the first import, write your redaction terms into config.toml. Both steps, with \
        the expected output: docs/runbooks/takeout-export.md"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply the pending database migrations. Running it again applies nothing.
    Migrate,
    /// Import Google Takeout exports: store the FreshDirect and restaurant-platform mail, then
    /// parse the new FreshDirect receipts. Idempotent: messages already stored are skipped.
    ///
    /// The path is a folder that holds the .zip archives as Google delivers them, one archive,
    /// or an unzipped Takeout/Mail/<label>.mbox file. Archives are read in place, one at a
    /// time; nothing is extracted. Through Compose the path starts with /data. The command needs the
    /// [redaction] section of config.toml and refuses to run without it. How to request the
    /// export and write the terms: docs/runbooks/takeout-export.md
    Import {
        /// A folder of Takeout archives, one archive, or one mailbox file.
        #[arg(long, visible_alias = "mbox", value_name = "PATH")]
        takeout: PathBuf,
    },
    /// What your next order most likely holds: the products most often in your latest
    /// orders. `--gap` and `--simple` are the two older rules.
    Due {
        /// How many products to list.
        #[arg(long, default_value_t = 15, value_name = "N")]
        top: usize,
        /// Use the gap rule instead: items whose usual gap between purchases has passed.
        #[arg(long, conflicts_with = "simple")]
        gap: bool,
        /// The day to judge from; today when omitted.
        #[arg(long, value_name = "YYYY-MM-DD")]
        as_of: Option<Date>,
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

const MAX_REFUSED_LINES: usize = 20;

pub fn render_import(import: &ImportReport, parse: &ParseReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "messages read {}, new {}, already known {}, from other senders {}\n",
        import.read, import.inserted, import.already_known, import.other_sender
    ));
    if !import.inserted_by_platform.is_empty() {
        let by_platform: Vec<String> = import
            .inserted_by_platform
            .iter()
            .map(|(platform, count)| format!("{platform} {count}"))
            .collect();
        out.push_str(&format!("new by platform: {}\n", by_platform.join(", ")));
    }
    for m in &import.malformed {
        out.push_str(&format!("malformed: {m}\n"));
    }
    // One line per message up to a screenful; beyond that the causes say more than the keys.
    if import.redaction_failures.len() <= MAX_REFUSED_LINES {
        for r in &import.redaction_failures {
            out.push_str(&format!("REFUSED, personal data survived redaction: {r}\n"));
        }
    }
    for (cause, count) in &import.refused_by_cause {
        out.push_str(&format!("REFUSED {count}: {cause}\n"));
    }
    out.push_str(&format!(
        "freshdirect receipts parsed {}, partial {}, failed {}, unsupported {}, left for later {}, anomalies {}",
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

fn habit_table(out: &mut String, rows: &[crate::application::recommend::HabitRow]) {
    if rows.is_empty() {
        out.push_str("none\n");
        return;
    }
    out.push_str(&format!(
        "{:<56} {:>8} {:>8} {:>11} {:>5}\n",
        "product", "recent", "longer", "last", "qty"
    ));
    for row in rows {
        let r = &row.recommendation;
        out.push_str(&format!(
            "{:<56} {:>8} {:>8} {:>11} {:>5}\n",
            truncate(&label(&row.product), 56),
            format!("{}/{}", r.in_recent, r.recent_orders),
            format!("{}/{}", r.in_long, r.long_orders),
            r.last_bought,
            r.typical_quantity
        ));
    }
}

pub fn render_habit(report: &HabitReport) -> String {
    let mut out = format!(
        "as of {}, {} orders; the products most often in your latest orders:\n",
        report.as_of, report.orders
    );
    habit_table(&mut out, &report.rows);
    if !report.stocked.is_empty() {
        out.push('\n');
    }
    for row in &report.stocked {
        let r = &row.recommendation;
        if let Some(until) = r.stocked_until {
            out.push_str(&format!(
                "stocked, left out: {} bought {}, expected to last until {}\n",
                truncate(&label(&row.product), 56),
                r.last_bought,
                until
            ));
        }
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
        for flag in ["--takeout", "--mbox"] {
            let cli = Cli::try_parse_from(["qm", "import", flag, "/tmp/export"]).expect("valid");
            assert!(
                matches!(cli.command, Command::Import { takeout } if takeout == *"/tmp/export")
            );
        }
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
