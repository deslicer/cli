//! Global `--timeout` / `--retries` (issue #226).

use clap::{CommandFactory, Parser};
use deslicer_cli::cli::Cli;

#[test]
fn top_level_help_documents_timeout_and_retries() {
    let mut buf = Vec::new();
    Cli::command()
        .write_long_help(&mut buf)
        .expect("write help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(
        help.contains("--timeout"),
        "top-level help missing --timeout:\n{help}"
    );
    assert!(
        help.contains("DESLICER_TIMEOUT") || help.contains("timeout"),
        "top-level help should mention timeout env:\n{help}"
    );
    assert!(
        help.contains("--retries"),
        "top-level help missing --retries:\n{help}"
    );
}

#[test]
fn timeout_and_retries_parse() {
    let cli = Cli::try_parse_from([
        "deslicer",
        "--timeout",
        "30",
        "--retries",
        "0",
        "auth",
        "status",
    ])
    .expect("parse");
    assert_eq!(cli.timeout, Some(30));
    assert_eq!(cli.retries, Some(0));
}
