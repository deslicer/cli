use clap::CommandFactory;
use deslicer_cli::cli::Cli; // pragma: allowlist secret

fn assert_non_empty_help(cmd: &clap::Command, path: &str) {
    if cmd.get_name() == "help" {
        return;
    }

    let about = cmd
        .get_about()
        .or_else(|| cmd.get_long_about())
        .map(|s| s.to_string())
        .unwrap_or_default();
    assert!(
        !about.trim().is_empty(),
        "empty about for `{path}` (command `{}`)",
        cmd.get_name()
    );

    for arg in cmd.get_arguments() {
        if arg.is_hide_set() {
            continue;
        }
        if arg.get_id() == "help" || arg.get_id() == "version" {
            continue;
        }
        if arg.get_long().is_none() && arg.get_short().is_none() && !arg.is_positional() {
            continue;
        }

        let help = arg
            .get_help()
            .or_else(|| arg.get_long_help())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let label = arg
            .get_long()
            .map(|s| format!("--{s}"))
            .or_else(|| arg.get_short().map(|s| format!("-{s}")))
            .unwrap_or_else(|| arg.get_id().to_string());
        assert!(
            !help.trim().is_empty(),
            "empty help for `{path}` argument `{label}`"
        );
    }

    for sub in cmd.get_subcommands() {
        let sub_path = if path.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{path} {}", sub.get_name())
        };
        assert_non_empty_help(sub, &sub_path);
    }
}

#[test]
fn every_command_and_flag_has_help_text() {
    let mut cmd = Cli::command();
    for name in ["auth", "change"] {
        let sub = cmd.find_subcommand_mut(name).expect(name);
        let path = format!("deslicer {name}"); // pragma: allowlist secret
        assert_non_empty_help(sub, &path);
    }
}

#[test]
fn change_status_help_documents_timeout() {
    let mut cmd = Cli::command();
    let change = cmd.find_subcommand_mut("change").expect("change");
    let status = change.find_subcommand_mut("status").expect("status");
    let mut buf = Vec::new();
    status.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(help.contains("--timeout-secs"));
    assert!(help.contains("Examples:"));
}
