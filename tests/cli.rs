//! Tests of the `qm` binary as a user starts it: the composition root, the reading of the
//! environment, and the exit status (REAS-TEST-001). Each test starts the program that cargo
//! built, with an environment that the test names in full and in a directory of its own. The
//! program looks for a `.env` file in its directory and then in each directory above it; an
//! empty one in the directory of the test ends that search, so no file and no variable of the
//! developer is read. The last test needs `DATABASE_URL`, like the tests of
//! `tests/database.rs`.
use std::path::Path;
use std::process::{Command, Output, Stdio};

use sqlx::PgPool;

/// Runs `qm` with the arguments and the variables, and nothing else in its environment. The
/// working directory and the home directory are `dir`, and the standard input is empty and
/// is no terminal.
fn qm(dir: &Path, variables: &[(&str, &str)], arguments: &[&str]) -> Output {
    std::fs::write(dir.join(".env"), "").expect("an empty .env that ends the search");
    Command::new(env!("CARGO_BIN_EXE_qm"))
        .args(arguments)
        .env_clear()
        .env("HOME", dir)
        .env("QM_CONFIG", dir.join("config.toml"))
        .envs(variables.iter().copied())
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("the program starts")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A database address that no test connects to: each command that gets it fails before.
const NO_DATABASE: (&str, &str) = ("DATABASE_URL", "postgres://nobody@127.0.0.1:1/none");

#[test]
fn a_command_without_a_database_address_fails_and_says_which_variable_is_missing() {
    let dir = tempfile::tempdir().expect("a directory");
    let output = qm(dir.path(), &[], &["due"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("DATABASE_URL"),
        "{}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn the_size_of_the_pool_is_a_positive_number_or_the_command_fails() {
    let dir = tempfile::tempdir().expect("a directory");
    for value in ["0", "-1", "many", ""] {
        let output = qm(
            dir.path(),
            &[NO_DATABASE, ("DB_MAX_CONNECTIONS", value)],
            &["gmail", "login"],
        );
        assert_eq!(output.status.code(), Some(1), "{value:?}");
        assert!(
            stderr(&output).contains("DB_MAX_CONNECTIONS"),
            "{value:?}: {}",
            stderr(&output)
        );
    }
    // The control: with a positive number the command goes on to its next question, which
    // here is the client of the Gmail API.
    for value in ["1", " 5 "] {
        let output = qm(
            dir.path(),
            &[NO_DATABASE, ("DB_MAX_CONNECTIONS", value)],
            &["gmail", "login"],
        );
        assert_eq!(output.status.code(), Some(1), "{value:?}");
        let text = stderr(&output);
        assert!(!text.contains("DB_MAX_CONNECTIONS"), "{value:?}: {text}");
        assert!(text.contains("QM_GMAIL_CLIENT_ID"), "{value:?}: {text}");
    }
}

#[test]
fn the_sign_in_needs_both_values_of_the_client_and_a_terminal() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = dir.path().join("state");
    let state = state.to_str().expect("a path of text");
    // One value, or a value of spaces only, is no client.
    for variables in [
        vec![("QM_GMAIL_CLIENT_ID", "id.apps.example")],
        vec![("QM_GMAIL_CLIENT_SECRET", "secret-1")],
        vec![
            ("QM_GMAIL_CLIENT_ID", "id.apps.example"),
            ("QM_GMAIL_CLIENT_SECRET", "  "),
        ],
        vec![
            ("QM_GMAIL_CLIENT_ID", " "),
            ("QM_GMAIL_CLIENT_SECRET", "secret-1"),
        ],
    ] {
        let mut all = vec![NO_DATABASE, ("QM_STATE", state)];
        all.extend(variables.iter().copied());
        let output = qm(dir.path(), &all, &["gmail", "login"]);
        assert_eq!(output.status.code(), Some(1), "{variables:?}");
        let text = stderr(&output);
        assert!(text.contains("QM_GMAIL_CLIENT_ID"), "{variables:?}: {text}");
        assert!(!text.contains("secret-1"), "{text}");
    }
    // With both values the command comes to the question that needs a person, and stops
    // there: the test gives it no terminal. It prints no address and no secret.
    let output = qm(
        dir.path(),
        &[
            NO_DATABASE,
            ("QM_STATE", state),
            ("QM_GMAIL_CLIENT_ID", "id.apps.example"),
            ("QM_GMAIL_CLIENT_SECRET", "secret-1"),
        ],
        &["gmail", "login"],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("run it in a terminal"), "{text}");
    assert!(!text.contains("QM_GMAIL_CLIENT_ID"), "{text}");
    assert!(!text.contains("secret-1"), "{text}");
    assert!(!text.contains("accounts.google.com"), "{text}");
    assert_eq!(stdout(&output), "");
}

#[test]
fn the_token_of_the_sign_in_is_looked_for_in_the_state_directory_that_is_set() {
    // `qm fetch` reads the token before it opens a connection, so its error names the place.
    let dir = tempfile::tempdir().expect("a directory");
    let client = [
        NO_DATABASE,
        ("QM_GMAIL_CLIENT_ID", "id.apps.example"),
        ("QM_GMAIL_CLIENT_SECRET", "secret-1"),
    ];
    let state = dir.path().join("chosen");
    let state_text = state.to_str().expect("a path of text");
    let mut with_state = client.to_vec();
    with_state.push(("QM_STATE", state_text));
    let output = qm(dir.path(), &with_state, &["fetch"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("no Gmail sign-in is stored at"), "{text}");
    assert!(
        text.contains(state.join("gmail-token.json").to_str().expect("text")),
        "{text}"
    );
    assert!(!text.contains("secret-1"), "{text}");

    // An empty value is no value: the place is then the state directory of the user, below
    // the home directory, and not the working directory.
    let mut with_empty = client.to_vec();
    with_empty.push(("QM_STATE", ""));
    let output = qm(dir.path(), &with_empty, &["fetch"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("no Gmail sign-in is stored at"), "{text}");
    assert!(!text.contains("chosen"), "{text}");
    assert!(
        text.contains(dir.path().join(".local").to_str().expect("text")),
        "{text}"
    );
}

#[test]
fn an_import_needs_no_redaction_terms_and_goes_on_to_the_database() {
    let dir = tempfile::tempdir().expect("a directory");
    // A configuration file with no `[redaction]` section. Until 2026-11-24 the import
    // stopped here and asked for the terms. It stores no text of a mail now, so it has
    // nothing to redact: it goes on to the database, which this test does not have.
    std::fs::write(
        dir.path().join("config.toml"),
        "[recommend]\nmin_purchases = 3\n",
    )
    .expect("the file");
    let output = qm(
        dir.path(),
        &[NO_DATABASE],
        &["import", "--takeout", "/no/such/export"],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(!text.contains("[redaction]"), "{text}");
    assert!(text.contains("database, connect"), "{text}");
    assert_eq!(stdout(&output), "");
}

#[test]
fn a_key_that_is_spelled_wrongly_in_the_redaction_section_stops_a_command() {
    let dir = tempfile::tempdir().expect("a directory");
    // `address_line` for `address_lines`, beside the five keys.
    std::fs::write(
        dir.path().join("config.toml"),
        "[redaction]\nnames = [\"Pat Example\"]\nemails = []\nphones = []\naddress_lines = []\n\
         card_last4 = []\naddress_line = [\"12 Example Street\"]\n",
    )
    .expect("the file");
    let output = qm(
        dir.path(),
        &[NO_DATABASE],
        &["import", "--takeout", "/no/such/export"],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("unknown field address_line"), "{text}");
    assert!(!text.contains("Example Street"), "{text}");
    assert!(!text.contains("/no/such/export"), "{text}");
    assert_eq!(stdout(&output), "");
    // A command that redacts nothing reads the same file and stops at the same key.
    let due = qm(dir.path(), &[NO_DATABASE], &["due"]);
    assert_eq!(due.status.code(), Some(1));
    let text = stderr(&due);
    assert!(text.contains("unknown field address_line"), "{text}");
    assert!(!text.contains("Example Street"), "{text}");
    assert_eq!(stdout(&due), "");
}

#[test]
fn an_addition_without_a_reason_and_without_a_terminal_is_refused() {
    let dir = tempfile::tempdir().expect("a directory");
    let output = qm(dir.path(), &[NO_DATABASE], &["add", "miso", "paste"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("say why with --reason"),
        "{}",
        stderr(&output)
    );
}

#[sqlx::test(migrations = false)]
async fn the_commands_reach_a_database_and_each_rule_of_due_prints_its_own_report(pool: PgPool) {
    // The address of the database of this test: the one of `DATABASE_URL` with the name of
    // the database that the test library made.
    let mut address =
        url::Url::parse(&std::env::var("DATABASE_URL").expect("DATABASE_URL")).expect("an address");
    let name = pool
        .connect_options()
        .get_database()
        .expect("a database name")
        .to_owned();
    address.set_path(&name);
    let address = address.to_string();
    let dir = tempfile::tempdir().expect("a directory");
    let run = |arguments: &'static [&'static str]| {
        let dir = dir.path().to_path_buf();
        let address = address.clone();
        // The program blocks, so it runs on a thread for blocking work.
        tokio::task::spawn_blocking(move || {
            qm(
                &dir,
                &[("DATABASE_URL", &address), ("DB_MAX_CONNECTIONS", "2")],
                arguments,
            )
        })
    };

    let first = run(&["migrate"]).await.expect("the task");
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert!(
        stdout(&first).ends_with("6 applied now, 0 already applied\n"),
        "{}",
        stdout(&first)
    );
    assert_eq!(stdout(&first).lines().count(), 7);
    let second = run(&["migrate"]).await.expect("the task");
    assert_eq!(stdout(&second), "0 applied now, 6 already applied\n");

    // The default is the habit rule; each of the two flags gives its own rule.
    let habit = run(&["due", "--as-of", "2026-03-27"])
        .await
        .expect("the task");
    assert_eq!(habit.status.code(), Some(0), "{}", stderr(&habit));
    assert!(
        stdout(&habit).starts_with("as of 2026-03-27, 0 orders\n\nESSENTIALS:"),
        "{}",
        stdout(&habit)
    );
    let simple = run(&["due", "--simple"]).await.expect("the task");
    assert_eq!(
        stdout(&simple),
        "0 orders; items in at least half of them and missing from the last:\nnone\n"
    );
    let gap = run(&["due", "--gap", "--as-of", "2026-03-27"])
        .await
        .expect("the task");
    assert_eq!(
        stdout(&gap),
        "as of 2026-03-27, 0 orders, usual gap between orders unknown\nnothing is due\n"
    );
}
