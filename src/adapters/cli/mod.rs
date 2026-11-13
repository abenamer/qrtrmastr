//! The command line, an inbound adapter: it turns arguments into use-case calls in `main.rs`.

use clap::{Parser, Subcommand};

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_parses() {
        let cli = Cli::try_parse_from(["qm", "migrate"]).expect("valid arguments");
        assert!(matches!(cli.command, Command::Migrate));
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        assert!(Cli::try_parse_from(["qm", "launch"]).is_err());
    }
}
