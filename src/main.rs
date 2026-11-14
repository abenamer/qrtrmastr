//! The composition root (REAS-ARCH-001): the one place that builds the concrete adapters and
//! wires them to the use cases. Every command fails with a non-zero exit status when its use
//! case fails (REAS-RUST-005).
#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use secrecy::ExposeSecret;
use tracing_subscriber::EnvFilter;

use qrtrmastr::adapters::cli::{Cli, Command};
use qrtrmastr::adapters::config::Settings;
use qrtrmastr::adapters::postgres::PostgresStore;
use qrtrmastr::application::migrate::Migrate;

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

async fn run(cli: Cli) -> anyhow::Result<()> {
    let settings = Settings::from_env()?;
    match cli.command {
        Command::Migrate => {
            let store = PostgresStore::connect(
                settings.database_url.expose_secret(),
                settings.db_max_connections,
            )
            .await?;
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
    }
}
