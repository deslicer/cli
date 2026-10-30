//! Regression: `docs --open` must not block launching a browser without a TTY.

use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn docs_quickstart_open_does_not_block_in_ci() {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .args(["docs", "quickstart", "--open"])
        .env("CI", "1")
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn docs quickstart --open");

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "docs --open blocked for {elapsed:?} without TTY"
    );
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("quickstart") || stdout.contains("docs."),
        "stdout should include the topic URL, got: {stdout}"
    );
}
