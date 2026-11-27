//! The composition root (REAS-ARCH-001): the one place that builds the concrete adapters and
//! wires them to the use cases. Every command fails with a non-zero exit status when its use
//! case fails (REAS-RUST-005). The clock is read here and passed in; the domain never reads it.

use std::process::ExitCode;

use clap::Parser;
use secrecy::ExposeSecret;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use qrtrmastr::adapters::cli::page::render_habit_page;
use qrtrmastr::adapters::cli::wire::{DueReportJson, HabitReportJson, SimpleReportJson};
use qrtrmastr::adapters::cli::{
    Cli, Command, GmailAction, ask_reason, json_text, log_line_is_safe, render_added, render_due,
    render_habit, render_import, render_simple, terminal_lines, terminal_text,
};
use qrtrmastr::adapters::config::{ConfigError, Settings};
use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::adapters::mail::gmail::{self, Endpoints, GmailApi, GmailError, SignIn, TokenFile};
use qrtrmastr::adapters::mail::takeout::TakeoutExport;
use qrtrmastr::adapters::postgres::PostgresStore;
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::import::ImportMail;
use qrtrmastr::application::list::ManageList;
use qrtrmastr::application::migrate::Migrate;
use qrtrmastr::application::parse::ParsePending;
use qrtrmastr::application::ports::{MailSource, RawMessageStore};
use qrtrmastr::application::recommend::{Due, DueOptions, HabitOptions};
use qrtrmastr::domain::ids::AdditionId;
use qrtrmastr::domain::list::ItemText;
use qrtrmastr::domain::recommend::days_between;
use qrtrmastr::domain::states::Platform;

fn main() -> ExitCode {
    // html5ever warns once per misplaced node in a mail's tables ("foster parenting not
    // implemented"); in a whole mailbox that is millions of lines and no defect. The warnings
    // stay hidden unless RUST_LOG names html5ever itself.
    let html5ever = if std::env::var("RUST_LOG").is_ok_and(|value| value.contains("html5ever")) {
        "html5ever=warn"
    } else {
        "html5ever=error"
    };
    #[expect(
        clippy::expect_used,
        reason = "the directive is one of the two constants above"
    )]
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"))
        .add_directive(html5ever.parse().expect("a constant directive is valid"));
    // A directive of RUST_LOG for a module of a library would win over the directive above,
    // so a second filter holds the libraries that print mail or tokens at a safe level,
    // whatever RUST_LOG asks (`log_line_is_safe`).
    let no_mail_text = filter_fn(|meta| log_line_is_safe(meta.target(), meta.level()));
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .with(no_mail_text)
        .init();
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {}", error_text(&error));
            ExitCode::FAILURE
        }
    }
}

async fn connect(settings: &Settings) -> anyhow::Result<PostgresStore> {
    Ok(PostgresStore::connect(
        settings.database_url.expose_secret(),
        settings.db_max_connections,
    )
    .await?)
}

fn token_file(settings: &Settings) -> anyhow::Result<TokenFile> {
    let dir = settings
        .state_dir
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no state directory; set QM_STATE"))?;
    Ok(TokenFile::in_dir(dir))
}

/// Stores the new messages of a source and parses the FreshDirect receipts that wait: what
/// `qm import` and `qm fetch` share.
async fn import_from(source: impl MailSource, settings: &Settings) -> anyhow::Result<()> {
    let reader = PlatformMail::new(settings.file.import.sender_platforms());
    let store = connect(settings).await?;
    // The pages of the receipts stay in memory from the import to the parse run and are not
    // stored: the store keeps what a parser extracts, not the mail (ADR 0021).
    let import = ImportMail::new(source, reader, store.clone())
        .keeping_pages_of(&[Platform::Freshdirect])
        .run()
        .await?;
    // The restaurant platforms have no parser yet (ADR 0007).
    let parse = ParsePending::new(store, Registry::default())
        .run(Platform::Freshdirect, &import.pages)
        .await?;
    print!("{}", render_import(&import, &parse));
    Ok(())
}

/// `qm gmail login`: the consent in the user's browser, then the refresh token into its file.
async fn gmail_login(settings: &Settings) -> anyhow::Result<()> {
    use std::io::{BufRead, IsTerminal};

    let credentials = settings.gmail.clone().ok_or(GmailError::NoCredentials)?;
    let file = token_file(settings)?;
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("`qm gmail login` asks you to paste an address; run it in a terminal");
    }
    let endpoints = Endpoints::default();
    let sign_in = SignIn::begin()?;
    eprintln!(
        "1. Open this address in a browser and approve read-only access to your mail:\n\n{}\n\n\
         2. The browser then goes to an address that starts with {} and shows an error page:\n   \
         nothing listens there, and that is expected.\n\
         3. Copy that whole address from the address bar, paste it here and press Enter.\n",
        sign_in.authorization_url(&endpoints, &credentials),
        gmail::REDIRECT_URI
    );
    let mut pasted = String::new();
    std::io::stdin().lock().read_line(&mut pasted)?;
    let code = sign_in.code_from_redirect(&pasted)?;
    let refresh = tokio::task::spawn_blocking(move || {
        sign_in.exchange(&gmail::agent(true), &endpoints, &credentials, &code)
    })
    .await??;
    file.save(&refresh)?;
    println!("signed in; `qm fetch` reads your new receipts");
    Ok(())
}

/// The text of a failed command for the terminal (REAS-SEC-006).
///
/// An error can quote a stored value, a path or an answer of a server, so its text is
/// encoded as one line. A configuration error is the exception: its text is the program's own,
/// it holds configuration only. Its line breaks stay; each line is still encoded. The error must be
/// the configuration error alone: with a context beside it, the text is one line like any other.
fn error_text(error: &anyhow::Error) -> String {
    let text = format!("{error:#}");
    if error.chain().len() == 1 && error.downcast_ref::<ConfigError>().is_some() {
        terminal_lines(&text)
    } else {
        terminal_text(&text)
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let settings = Settings::load()?;
    match cli.command {
        Command::Migrate => {
            let store = connect(&settings).await?;
            let report = Migrate::new(store).run().await?;
            for applied in &report.applied {
                println!("applied {}", terminal_text(&applied.to_string()));
            }
            println!(
                "{} applied now, {} already applied",
                report.applied.len(),
                report.already_applied
            );
            Ok(())
        }
        Command::Import { takeout } => import_from(TakeoutExport::new(takeout), &settings).await,
        Command::Gmail {
            action: GmailAction::Login,
        } => gmail_login(&settings).await,
        Command::Fetch { since, max } => {
            let credentials = settings.gmail.clone().ok_or(GmailError::NoCredentials)?;
            let refresh = token_file(&settings)?.load()?;
            // A week of overlap costs a few known messages and covers a late delivery.
            let after = match since {
                Some(day) => Some(day.to_zoned_datetime(jiff::tz::TimeZone::UTC)?.timestamp()),
                None => connect(&settings)
                    .await?
                    .newest_received_at()
                    .await?
                    // The time of a message is the Date header its sender wrote: one in the future
                    // must not move the search past today.
                    .map(|newest| {
                        newest
                            .min(jiff::Timestamp::now())
                            .saturating_sub(jiff::SignedDuration::from_hours(7 * 24))
                    })
                    .transpose()?,
            };
            let senders: Vec<String> = settings
                .file
                .import
                .sender_platforms()
                .into_iter()
                .map(|(sender, _)| sender)
                .collect();
            let endpoints = Endpoints::default();
            let agent = gmail::agent(true);
            // The HTTP client blocks; keep it off the executor (REAS-RUST-007).
            let access = {
                let (agent, endpoints) = (agent.clone(), endpoints.clone());
                tokio::task::spawn_blocking(move || {
                    gmail::access_token(&agent, &endpoints, &credentials, &refresh)
                })
                .await??
            };
            let source = GmailApi::new(
                agent,
                &endpoints,
                access,
                gmail::search_query(&senders, after),
                max,
            );
            import_from(source, &settings).await
        }
        Command::Due {
            as_of,
            include_tentative,
            include_lapsed,
            include_not_due,
            simple,
            json,
            top,
            gap,
            remember,
            seed,
            html,
        } => {
            let store = connect(&settings).await?;
            let due = Due::new(store, settings.file.recommend.thresholds());
            let as_of = as_of.unwrap_or_else(|| jiff::ZonedDateTime::now().date());
            if !simple && !gap {
                // The habit rule is the default (ADR 0009).
                // The shuffle of the reminders is fixed for a day unless a seed is given.
                let options = HabitOptions {
                    extras: top,
                    remember,
                    seed: seed.unwrap_or_else(|| {
                        // The day as a number: the days since the first day of year 1, that day counted.
                        days_between(jiff::civil::date(1, 1, 1), as_of)
                            .saturating_add(1)
                            .unsigned_abs()
                    }),
                };
                let report = due.run_habit(options, as_of).await?;
                if html {
                    print!("{}", render_habit_page(&report));
                } else if json {
                    println!("{}", json_text(&HabitReportJson::from(&report))?);
                } else {
                    print!("{}", render_habit(&report));
                }
            } else if simple {
                let report = due.run_simple().await?;
                if json {
                    println!("{}", json_text(&SimpleReportJson::from(&report))?);
                } else {
                    print!("{}", render_simple(&report));
                }
            } else {
                let options = DueOptions {
                    include_tentative,
                    include_lapsed,
                    include_not_due,
                };
                let report = due.run(as_of, options).await?;
                if json {
                    println!("{}", json_text(&DueReportJson::from(&report))?);
                } else {
                    print!("{}", render_due(&report));
                }
            }
            Ok(())
        }
        Command::Add { item, reason } => {
            let item = ItemText::parse(&item.join(" "))?;
            // Every addition carries its reason (ADR 0011): asked for when it is not given.
            let reason = match reason {
                Some(reason) => reason,
                None if std::io::IsTerminal::is_terminal(&std::io::stdin()) => {
                    ask_reason(item.as_str(), std::io::stdin().lock(), std::io::stderr())?
                        .ok_or_else(|| anyhow::anyhow!("no reason given; nothing was added"))?
                }
                None => anyhow::bail!(
                    "say why with --reason: out-of-stock, running-low, recipe, new-food or repeat-favorite"
                ),
            };
            let store = connect(&settings).await?;
            let today = jiff::ZonedDateTime::now().date();
            let added = ManageList::new(store).add(item, reason, today).await?;
            print!("{}", render_added(&added));
            Ok(())
        }
        Command::Drop { id } => {
            let store = connect(&settings).await?;
            if ManageList::new(store).drop(AdditionId::new(id)?).await? {
                println!("dropped {id}");
                Ok(())
            } else {
                anyhow::bail!("no addition has the number {id}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_configuration_error_keeps_its_lines_and_any_other_error_is_one_line() {
        // A configuration error is the program's own text with a path in it. The path is
        // outside data: a control character in it is shown as text.
        let file_error = |path: &str| ConfigError::File {
            path: PathBuf::from(path),
            reason: "unknown variant at import.restaurant_senders".to_owned(),
        };
        let shown = error_text(&anyhow::Error::from(file_error("/config/a\u{1b}[2J.toml")));
        assert!(
            shown.starts_with("cannot read the configuration file /config/a\\u{1b}[2J.toml:"),
            "{shown:?}"
        );
        assert!(!shown.contains('\u{1b}'));

        // A configuration error with a context is not the program's own text alone.
        let with_context = anyhow::Error::from(file_error("/config/config.toml"))
            .context("stored value x\nerror: forged");
        let shown = error_text(&with_context);
        assert!(!shown.contains('\n'), "{shown:?}");
        assert!(
            shown.starts_with("stored value x\\u{a}error: forged: cannot read the configuration"),
            "{shown:?}"
        );

        // Any other error can quote outside data: a line break in it would forge a line.
        let other = anyhow::anyhow!("row 7 holds \"x\nerror: forged\u{1b}[2J\"");
        let shown = error_text(&other.context("list the purchases"));
        assert_eq!(
            shown,
            "list the purchases: row 7 holds \"x\\u{a}error: forged\\u{1b}[2J\""
        );
    }
}
