//! Closed stdout must not panic (agent pipes like `deslicer --help | head -1`).

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn help_closed_stdout_does_not_panic() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn deslicer --help");

    // Match `head -1`: read one line, then close the pipe so further writes get EPIPE.
    {
        let stdout = child.stdout.take().expect("stdout pipe");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
    }

    let output = child.wait_with_output().expect("wait for deslicer");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "closed stdout must not panic; stderr was:\n{stderr}"
    );
    assert!(
        !stderr.contains("clap error print"),
        "closed stdout must not surface clap print expect; stderr was:\n{stderr}"
    );

    // Quiet exit (0) after ignoring EPIPE, or 141 / SIGPIPE like other Unix tools.
    if let Some(code) = output.status.code() {
        assert!(
            code == 0 || code == 141,
            "unexpected exit {code}; stderr:\n{stderr}"
        );
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // SIGPIPE is 13 on Linux, macOS, and other common Unix targets.
        assert_eq!(
            output.status.signal(),
            Some(13),
            "expected SIGPIPE when no exit code; status={:?} stderr:\n{stderr}",
            output.status
        );
    }
}
