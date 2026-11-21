//! The command line, an inbound adapter: it turns arguments into use-case calls in `main.rs`
//! and renders the reports the use cases return.

pub mod page;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use jiff::civil::Date;

use crate::application::import::ImportReport;
use crate::application::list::Added;
use crate::application::parse::ParseReport;
use crate::application::recommend::{DueReport, HabitReport, SimpleReport};
use crate::domain::recommend::Status;
use crate::domain::states::AddReason;

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
    /// or an unzipped `Takeout/Mail/<label>.mbox` file. Archives are read in place, one at a
    /// time; nothing is extracted. Through Compose the path starts with /data. The command needs the
    /// `[redaction]` section of config.toml and refuses to run without it. How to request the
    /// export and write the terms: docs/runbooks/takeout-export.md
    Import {
        /// A folder of Takeout archives, one archive, or one mailbox file.
        #[arg(long, visible_alias = "mbox", value_name = "PATH")]
        takeout: PathBuf,
    },
    /// Read the new receipts straight from Gmail, then store and parse them as `qm import`
    /// does. Needs `qm gmail login` once. Reads the mail of the configured senders received
    /// since a week before the newest stored message; known messages are skipped.
    Fetch {
        /// Read mail received since this day instead.
        #[arg(long, value_name = "YYYY-MM-DD")]
        since: Option<Date>,
        /// The most messages one run reads.
        #[arg(long, default_value_t = 500, value_name = "N")]
        max: usize,
    },
    /// Sign in to Gmail for `qm fetch`.
    Gmail {
        #[command(subcommand)]
        action: GmailAction,
    },
    /// What your next order most likely holds: the essentials (in a quarter of your latest
    /// orders), some extras, and a few things you used to buy. `--gap` and `--simple` are the
    /// two older rules.
    Due {
        /// How many extras to list after the essentials.
        #[arg(long, default_value_t = 10, value_name = "N")]
        top: usize,
        /// How many "remember this?" products to list.
        #[arg(long, default_value_t = 5, value_name = "N")]
        remember: usize,
        /// The seed of the "remember this?" shuffle; the day when omitted, so the list
        /// changes each day. Give another number for another list.
        #[arg(long, value_name = "NUMBER")]
        seed: Option<u64>,
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
        /// Print a web page instead of a table: `qm due --html > ~/next-order.html`, then open
        /// the file in a browser. Not with --gap or --simple.
        #[arg(long, conflicts_with_all = ["json", "gap", "simple"])]
        html: bool,
    },
    /// Add an item to your next order by hand, with the reason. `qm due` lists your additions
    /// until an order is delivered after them. Without --reason the command asks.
    Add {
        /// What to add, as you would search for it: `qm add miso paste`.
        #[arg(required = true, value_name = "ITEM")]
        item: Vec<String>,
        /// Why: out-of-stock (you ran out), running-low (almost out), recipe, new-food or
        /// repeat-favorite.
        #[arg(long, value_name = "REASON")]
        reason: Option<AddReason>,
    },
    /// Remove one of your additions; the number is the one `qm due` prints before it.
    Drop {
        #[arg(value_name = "NUMBER")]
        id: i64,
    },
    /// Work with parser fixtures.
    Fixture {
        #[command(subcommand)]
        action: FixtureAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum GmailAction {
    /// Give the tool read-only access to your mail. It prints an address to open in a
    /// browser, you approve, and you paste back the address the browser ends on. The sign-in
    /// is kept in a file only you can read. Setup: docs/runbooks/gmail-fetch.md
    Login,
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

/// Whether a log line of `target` at `level` may be printed, whatever `RUST_LOG` asks for.
/// Two libraries print what this tool must never log. html5ever prints the tokens of a mail
/// before redaction at debug and trace. ureq prints every byte it sends and receives at
/// trace: the Gmail tokens and the raw messages among them. The warnings of html5ever and
/// the debug lines of ureq carry neither. Every other target follows `RUST_LOG`.
pub fn log_line_is_safe(target: &str, level: &tracing::Level) -> bool {
    if target.starts_with("html5ever") {
        *level <= tracing::Level::WARN
    } else if target.starts_with("ureq") {
        *level <= tracing::Level::DEBUG
    } else {
        true
    }
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
        out.push_str(&format!("malformed: {}\n", terminal_text(m)));
    }
    // One line per message up to a screenful; beyond that the causes say more than the keys.
    if import.redaction_failures.len() <= MAX_REFUSED_LINES {
        for r in &import.redaction_failures {
            out.push_str(&format!(
                "REFUSED, personal data survived redaction: {}\n",
                terminal_text(r)
            ));
        }
    }
    for (cause, count) in &import.refused_by_cause {
        out.push_str(&format!("REFUSED {count}: {}\n", terminal_text(cause)));
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

/// A text as one piece of a line of the terminal (REAS-SEC-006).
///
/// The terminal is an output context with its own syntax: a control character moves the
/// cursor, clears the screen or starts an escape sequence, and a direction mark reorders what
/// the reader sees. A product name comes from a receipt and an error can quote a stored value,
/// so both are outside data here. Each such character is written as `\u{..}`, which the
/// terminal shows as text; nothing is removed, so the reader sees that it was there.
///
/// The encoding belongs to this sink only. `qm due --json` and the page of `qm due --html`
/// have their own, and the stored text stays as it is. A backslash is not encoded, so a name
/// that holds the six characters `\u{1b}` reads like an encoded one; nothing follows from it.
pub fn terminal_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if acts_on_a_terminal(character) {
            out.extend(character.escape_unicode());
        } else {
            out.push(character);
        }
    }
    out
}

/// A text of several lines that the program itself wrote, such as the help of a
/// configuration error: each line is encoded as [`terminal_text`] does, and the line breaks
/// stay. Not for a text that can hold outside data, where a line break could forge a line.
pub fn terminal_lines(text: &str) -> String {
    text.split('\n')
        .map(terminal_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a terminal obeys the character or hides it from the reader: a control character,
/// a mark that changes the direction of the text after it, one of the two separators that a
/// terminal can take for a line break, or a tag character, which no terminal shows and which
/// a program that reads the output still receives. The characters that join an emoji or a
/// script (the zero-width joiners, the variation selectors) are not in the set: text needs
/// them, and they move nothing.
fn acts_on_a_terminal(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{61c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{e0000}'..='\u{e007f}'
        )
}

/// A report as JSON for `--json` (REAS-SEC-006).
///
/// `serde_json` writes the control characters below U+0020 as escapes, which JSON requires,
/// and every other character as itself. Standard output is often a terminal, so this
/// function also writes as `\uXXXX` each other character that a terminal obeys or hides. The
/// result is the same JSON value: a reader decodes each escape back to the character.
///
/// The pass works on the serialized text. That is correct because `serde_json` writes ASCII
/// only outside a string, and inside a string it never writes a line break as itself; the
/// only characters the pass changes are therefore inside strings. A `serde_json` raw value
/// would break that, because it is written as it is; no report holds one.
///
/// # Errors
///
/// The error of `serde_json` when the value cannot be serialized.
pub fn json_text<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let pretty = serde_json::to_string_pretty(value)?;
    let mut out = String::with_capacity(pretty.len());
    let mut units = [0u16; 2];
    for character in pretty.chars() {
        if character != '\n' && acts_on_a_terminal(character) {
            for unit in character.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            out.push(character);
        }
    }
    Ok(out)
}

/// The name of a product with its size, as the receipt printed them. Each output context
/// encodes it for itself: the tables through [`label`], the page through its own `escape`.
fn label_text(row_product: &crate::application::ports::ProductLabel) -> String {
    match &row_product.size_text {
        Some(size) => format!("{} ({size})", row_product.display_name),
        None => row_product.display_name.clone(),
    }
}

/// The name of a product with its size, for a table on the terminal.
fn label(row_product: &crate::application::ports::ProductLabel) -> String {
    terminal_text(&label_text(row_product))
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
    let mut out = format!("as of {}, {} orders\n", report.as_of, report.orders);
    out.push_str("\nESSENTIALS: in at least a quarter of your latest orders\n");
    habit_table(&mut out, &report.essentials);
    out.push_str("\nEXTRAS: the next most often in your latest orders\n");
    habit_table(&mut out, &report.extras);
    out.push_str(&format!(
        "\nREMEMBER THIS? bought before, not in your latest orders (--seed {})\n",
        report.seed
    ));
    if report.remember.is_empty() {
        out.push_str("none\n");
    } else {
        out.push_str(&format!(
            "{:<56} {:>8} {:>11}\n",
            "product", "orders", "last"
        ));
    }
    for row in &report.remember {
        out.push_str(&format!(
            "{:<56} {:>8} {:>11}\n",
            truncate(&label(&row.product), 56),
            row.reminder.orders_with_product,
            row.reminder.last_bought
        ));
    }
    out.push_str("\nYOUR ADDITIONS: qm add ITEM, qm drop NUMBER\n");
    if report.additions.is_empty() {
        out.push_str("none\n");
    }
    for addition in &report.additions {
        out.push_str(&format!(
            "{:>4}  {:<50} {:<16} {:>11}\n",
            addition.id,
            truncate(&terminal_text(addition.item.as_str()), 50),
            addition.reason.phrase(),
            addition.added_on
        ));
    }
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

pub fn render_added(added: &Added) -> String {
    let product = match &added.product {
        Some(product) => format!("; in your history as {}", label(product)),
        None => String::new(),
    };
    format!(
        "added {}: {} ({}){product}\n",
        added.id,
        terminal_text(added.item.as_str()),
        added.reason.phrase()
    )
}

/// Asks why an item is added, until an answer names a reason: its number or its text. `None`
/// when the input ends first.
pub fn ask_reason(
    item: &str,
    mut input: impl std::io::BufRead,
    mut output: impl std::io::Write,
) -> std::io::Result<Option<AddReason>> {
    writeln!(output, "Why are you adding {}?", terminal_text(item))?;
    for (number, reason) in (1_usize..).zip(AddReason::ALL) {
        writeln!(output, "  {number}  {}", reason.phrase())?;
    }
    loop {
        write!(output, "1-{}> ", AddReason::ALL.len())?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let answer = line.trim().to_lowercase();
        let by_number = answer
            .parse::<usize>()
            .ok()
            .and_then(|n| AddReason::ALL.get(n.checked_sub(1)?));
        let by_text = AddReason::ALL
            .iter()
            .find(|r| r.as_str() == answer || r.phrase() == answer);
        if let Some(reason) = by_number.or(by_text) {
            return Ok(Some(*reason));
        }
    }
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
        let cut: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{cut}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_setting_lets_a_library_print_mail_or_tokens() {
        use tracing::Level;
        // The HTML parser: its module targets too, at debug and trace.
        for target in [
            "html5ever",
            "html5ever::tokenizer",
            "html5ever::tree_builder",
        ] {
            assert!(log_line_is_safe(target, &Level::WARN), "{target}");
            assert!(log_line_is_safe(target, &Level::ERROR), "{target}");
            assert!(!log_line_is_safe(target, &Level::INFO), "{target}");
            assert!(!log_line_is_safe(target, &Level::DEBUG), "{target}");
            assert!(!log_line_is_safe(target, &Level::TRACE), "{target}");
        }
        // The HTTP client and its protocol crate: trace is the hex dump of every byte.
        for target in ["ureq", "ureq::run", "ureq_proto", "ureq_proto::util"] {
            assert!(log_line_is_safe(target, &Level::DEBUG), "{target}");
            assert!(!log_line_is_safe(target, &Level::TRACE), "{target}");
        }
        for target in [
            "qrtrmastr",
            "sqlx::query",
            "qrtrmastr::adapters::mail::takeout",
        ] {
            assert!(log_line_is_safe(target, &Level::TRACE), "{target}");
        }
    }

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
        let cli = Cli::try_parse_from(["qm", "gmail", "login"]).expect("valid");
        assert!(matches!(
            cli.command,
            Command::Gmail {
                action: GmailAction::Login
            }
        ));
        let cli = Cli::try_parse_from(["qm", "fetch"]).expect("valid");
        assert!(matches!(
            cli.command,
            Command::Fetch {
                since: None,
                max: 500
            }
        ));
        let cli = Cli::try_parse_from(["qm", "fetch", "--since", "2026-11-01", "--max", "20"])
            .expect("valid");
        assert!(matches!(
            cli.command,
            Command::Fetch {
                since: Some(_),
                max: 20
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

    #[test]
    fn a_text_for_the_terminal_keeps_its_letters_and_shows_its_controls_as_text() {
        // Letters of any script, signs and spaces stay as they are.
        for plain in [
            "Whole Milk (1/2 gallon)",
            "Cr\u{e8}me Fra\u{ee}che, 8oz",
            "\u{8c46}\u{8150} tofu \u{1f34b} 50% off & more",
            "",
        ] {
            assert_eq!(terminal_text(plain), plain);
        }
        // Each character that a terminal obeys is written as text, and nothing is removed.
        assert_eq!(terminal_text("a\u{1b}[2Jb"), "a\\u{1b}[2Jb");
        assert_eq!(terminal_text("a\nb\rc\td"), "a\\u{a}b\\u{d}c\\u{9}d");
        assert_eq!(terminal_text("\u{7}\u{8}\u{7f}"), "\\u{7}\\u{8}\\u{7f}");
        // The one-character form of the escape sequence starter, in the second control block.
        assert_eq!(terminal_text("x\u{9b}31m"), "x\\u{9b}31m");
        // Direction marks and the two separators are not control characters for `char`.
        assert_eq!(terminal_text("a\u{202e}b"), "a\\u{202e}b");
        assert_eq!(
            terminal_text("\u{2066}\u{2069}\u{200f}"),
            "\\u{2066}\\u{2069}\\u{200f}"
        );
        assert_eq!(
            terminal_text("a\u{2028}b\u{2029}c"),
            "a\\u{2028}b\\u{2029}c"
        );
        // The characters on each side of the two ranges stay.
        assert_eq!(terminal_text("\u{2029}\u{202a}"), "\\u{2029}\\u{202a}");
        assert_eq!(
            terminal_text("\u{202f}\u{2065}\u{206a}"),
            "\u{202f}\u{2065}\u{206a}"
        );
        assert_eq!(terminal_text("\u{a0}\u{1f}\u{20}"), "\u{a0}\\u{1f} ");
        // A tag character: no terminal shows it, and a program that reads the output gets it.
        assert_eq!(
            terminal_text("a\u{e0041}\u{e007f}b"),
            "a\\u{e0041}\\u{e007f}b"
        );
        assert_eq!(terminal_text("\u{dffff}\u{e0080}"), "\u{dffff}\u{e0080}");
        // What joins an emoji stays: a family of two, and a sign with its variation selector.
        let joined = "\u{1f468}\u{200d}\u{1f467} \u{2764}\u{fe0f}";
        assert_eq!(terminal_text(joined), joined);
    }

    #[test]
    fn a_text_of_several_lines_keeps_its_line_breaks_and_nothing_else() {
        assert_eq!(terminal_lines("one\ntwo\n\nfour"), "one\ntwo\n\nfour");
        assert_eq!(
            terminal_lines("a\u{1b}[2J\nb\rc\n\u{202e}d"),
            "a\\u{1b}[2J\nb\\u{d}c\n\\u{202e}d"
        );
        assert_eq!(terminal_lines(""), "");
    }

    #[test]
    fn the_import_report_shows_a_control_character_of_a_message_key_as_text() {
        // A Message-ID is read before the sender check, and a mail library keeps a folded
        // line break inside it. The second key forges a line of the report.
        let hostile = "a\u{1b}[2J@x.example";
        let forged = "b\r\n messages read 0, new 0@x.example";
        let import = ImportReport {
            read: 2,
            inserted: 0,
            inserted_by_platform: std::collections::BTreeMap::new(),
            already_known: 0,
            other_sender: 0,
            malformed: vec![hostile.to_owned(), forged.to_owned()],
            redaction_failures: vec![hostile.to_owned(), forged.to_owned()],
            refused_by_cause: std::collections::BTreeMap::from([(
                "freshdirect receipt, text part: a name in the text".to_owned(),
                2,
            )]),
        };
        let report = render_import(&import, &ParseReport::default());
        assert!(
            !report.contains(|c: char| c.is_control() && c != '\n'),
            "{report:?}"
        );
        // The first line, two lines for each of the two lists, the cause, the parse line.
        assert_eq!(report.lines().count(), 7, "{report:?}");
        assert!(report.contains("malformed: a\\u{1b}[2J@x.example\n"));
        assert!(report.contains(
            "REFUSED, personal data survived redaction: b\\u{d}\\u{a} messages read 0, new 0@x.example\n"
        ));
        // A cause is the program's own text and is printed as it is.
        assert!(report.contains("REFUSED 2: freshdirect receipt, text part: a name in the text\n"));
    }

    #[test]
    fn a_report_as_json_has_no_character_that_a_terminal_obeys_and_is_the_same_value() {
        // Every class that `serde_json` leaves as the character itself, and two that it
        // escapes: the delete character, the second control block, a direction mark, the line
        // separators, a tag character; a tab and an escape character.
        let text = "a\u{7f}b\u{9b}31m\u{85}c\u{202e}d\u{2028}e\u{2029}f\u{e0041}g\th\u{1b}i \"q\" \\ \u{e9}\u{1f34b}";
        let value = serde_json::json!({ "name": text, "list": [text, "plain"], "n": 3 });
        let json = json_text(&value).expect("json");

        assert!(
            !json.contains(|c: char| acts_on_a_terminal(c) && c != '\n'),
            "{json:?}"
        );
        assert!(json.contains(
            "a\\u007fb\\u009b31m\\u0085c\\u202ed\\u2028e\\u2029f\\udb40\\udc41g\\th\\u001bi"
        ));
        // Letters and signs that are not in the set stay as they are.
        assert!(json.contains("\u{e9}\u{1f34b}"));
        // The same value comes back, and the layout is the one of the pretty printer.
        let back: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(back, value);
        assert_eq!(
            json.lines().count(),
            serde_json::to_string_pretty(&value)
                .expect("json")
                .lines()
                .count()
        );
    }

    #[test]
    fn the_line_of_an_added_item_shows_a_direction_mark_as_text() {
        use crate::domain::ids::AdditionId;
        use crate::domain::list::ItemText;

        // An item text refuses control characters when it is made, and admits a direction mark.
        let added = Added {
            id: AdditionId::new(4).expect("id"),
            item: ItemText::parse("miso \u{202e}etsap").expect("text"),
            reason: AddReason::NewFood,
            product: None,
        };
        assert_eq!(
            render_added(&added),
            "added 4: miso \\u{202e}etsap (new food to try)\n"
        );
    }

    #[test]
    fn the_reason_prompt_shows_a_control_character_of_the_item_as_text() {
        let mut shown = Vec::new();
        ask_reason("miso\u{1b}[2J\npaste", "1\n".as_bytes(), &mut shown).expect("io");
        let shown = String::from_utf8(shown).expect("utf-8");
        assert!(
            shown.starts_with("Why are you adding miso\\u{1b}[2J\\u{a}paste?\n"),
            "{shown:?}"
        );
        assert!(!shown.contains('\u{1b}'));
    }

    #[test]
    fn the_reason_prompt_takes_a_number_or_the_text_and_asks_again_otherwise() {
        let ask = |answers: &str| {
            let mut shown = Vec::new();
            let reason = ask_reason("miso paste", answers.as_bytes(), &mut shown).expect("io");
            (reason, String::from_utf8(shown).expect("utf-8"))
        };
        let (reason, shown) = ask("3\n");
        assert_eq!(reason, Some(AddReason::Recipe));
        assert_eq!(ask("2\n").0, Some(AddReason::RunningLow));
        assert_eq!(ask("almost running out\n").0, Some(AddReason::RunningLow));
        assert!(shown.contains("Why are you adding miso paste?"));
        assert!(shown.contains("  1  ran out, need to replace"));
        assert!(shown.contains("  2  almost running out"));
        assert!(shown.contains("  5  repeat favorite"));
        assert_eq!(ask("new-food\n").0, Some(AddReason::NewFood));
        assert_eq!(ask("Out-Of-Stock\n").0, Some(AddReason::OutOfStock));
        // A wrong answer is asked again; the end of the input gives no reason.
        let (reason, shown) = ask("because\n0\n9\n5\n");
        assert_eq!(reason, Some(AddReason::RepeatFavorite));
        assert_eq!(shown.matches("1-5> ").count(), 4);
        assert_eq!(ask("because\n").0, None);
    }

    #[test]
    fn a_text_is_cut_to_a_width_and_the_cut_is_marked() {
        assert_eq!(truncate("milk", 5), "milk");
        assert_eq!(truncate("milk", 4), "milk");
        assert_eq!(truncate("milk", 3), "mi\u{2026}");
        assert_eq!(truncate("milk", 1), "\u{2026}");
        assert_eq!(truncate("", 0), "");
    }

    #[test]
    fn a_width_of_zero_gives_the_mark_alone_and_does_not_stop_the_table() {
        assert_eq!(truncate("milk", 0), "\u{2026}");
    }
}
