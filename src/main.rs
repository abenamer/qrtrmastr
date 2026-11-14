//! The composition root (REAS-ARCH-001): the one place that builds the concrete adapters and
//! wires them to the use cases. Every command fails with a non-zero exit status when its use
//! case fails (REAS-RUST-005). The clock is read here and passed in; the domain never reads it.
#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use secrecy::ExposeSecret;
use tracing_subscriber::EnvFilter;

use qrtrmastr::adapters::cli::{
    Cli, Command, FixtureAction, render_due, render_import, render_simple,
};
use qrtrmastr::adapters::config::Settings;
use qrtrmastr::adapters::fixtures::{anchor, write_fixture};
use qrtrmastr::adapters::mail::FreshDirectMail;
use qrtrmastr::adapters::mail::mbox::MboxFile;
use qrtrmastr::adapters::mail::redact::Redactor;
use qrtrmastr::adapters::postgres::PostgresStore;
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::fixture::ExportFixture;
use qrtrmastr::application::import::ImportMail;
use qrtrmastr::application::migrate::Migrate;
use qrtrmastr::application::parse::ParsePending;
use qrtrmastr::application::recommend::{Due, DueOptions};
use qrtrmastr::domain::states::Platform;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
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
            eprintln!("error: {error:#}");
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

async fn run(cli: Cli) -> anyhow::Result<()> {
    let settings = Settings::load()?;
    match cli.command {
        Command::Migrate => {
            let store = connect(&settings).await?;
            let report = Migrate::new(store).run().await?;
            for applied in &report.applied {
                println!("applied {applied}");
            }
            println!(
                "{} applied now, {} already applied",
                report.applied.len(),
                report.already_applied
            );
            Ok(())
        }
        Command::Import { mbox } => {
            let terms = settings.redaction_terms()?;
            let redactor = Redactor::new(terms)?;
            let reader = FreshDirectMail::new(settings.file.import.senders.clone(), redactor);
            let store = connect(&settings).await?;
            let import = ImportMail::new(MboxFile::new(mbox), reader, store.clone())
                .run()
                .await?;
            let parse = ParsePending::new(store, Registry::default())
                .run(Platform::Freshdirect)
                .await?;
            print!("{}", render_import(&import, &parse));
            if import.redaction_failures.is_empty() {
                Ok(())
            } else {
                anyhow::bail!(
                    "{} message(s) refused because personal data survived redaction; nothing of them was stored",
                    import.redaction_failures.len()
                )
            }
        }
        Command::Due {
            as_of,
            include_tentative,
            include_lapsed,
            include_not_due,
            simple,
            json,
        } => {
            let store = connect(&settings).await?;
            let due = Due::new(store, settings.file.recommend.thresholds());
            if simple {
                let report = due.run_simple().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print!("{}", render_simple(&report));
                }
            } else {
                let as_of = as_of.unwrap_or_else(|| chrono::Local::now().date_naive());
                let options = DueOptions {
                    include_tentative,
                    include_lapsed,
                    include_not_due,
                };
                let report = due.run(as_of, options).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print!("{}", render_due(&report));
                }
            }
            Ok(())
        }
        Command::Fixture {
            action: FixtureAction::Add { key, dir },
        } => {
            let store = connect(&settings).await?;
            let dir = anchor(&dir, &std::env::current_dir()?);
            match ExportFixture::new(store).run(&key).await? {
                None => anyhow::bail!("no stored message has the key {key}"),
                Some(body) => match write_fixture(&dir, &body)? {
                    None => anyhow::bail!("message {key} has no HTML body to write"),
                    Some(path) => {
                        println!("wrote {}", path.display());
                        Ok(())
                    }
                },
            }
        }
    }
}
