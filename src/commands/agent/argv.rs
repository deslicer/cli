//! Rewrites `deslicer agent` argv so a missing subcommand is a REPL,
//! and a bare prompt is `run`.
//!
//! Clap cannot express "first token is either a subcommand or a prompt"
//! without `external_subcommand`, which would swallow `list` / `ls`. This
//! rewrite runs before parse and only inserts `repl` or `run`.

use std::ffi::{OsStr, OsString};

const AGENT: &str = "agent";

const SUBCOMMANDS: &[&str] = &["list", "ls", "run", "logs", "resume", "repl", "help"];

const VALUE_FLAGS: &[&str] = &[
    "-a",
    "--agent",
    "--conversation",
    "--idempotency-key",
    "--deslicer-api-url",
    "--observer-api-url",
    "--ci-platform",
    "--log-format",
];

/// Inserts `repl` or `run` after `agent` when the user omitted a subcommand.
pub fn rewrite_agent_argv<I, S>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let Some(agent_at) = args.iter().position(|arg| arg == AGENT) else {
        return args;
    };

    let after = &args[agent_at + 1..];
    match first_positional(after) {
        Some(token) if is_subcommand(token) => args,
        Some(_) => insert_after(&args, agent_at, "run"),
        // `deslicer agent --help` must list subcommands, not the REPL's flags.
        None if has_help_flag(after) => args,
        None => insert_after(&args, agent_at, "repl"),
    }
}

fn first_positional(args: &[OsString]) -> Option<&OsStr> {
    let mut i = 0;
    while i < args.len() {
        let token = args[i].to_string_lossy();
        if token == "--" {
            return args.get(i + 1).map(OsString::as_os_str);
        }
        if token.starts_with('-') {
            if !token.contains('=') && takes_value(&token) {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        return Some(args[i].as_os_str());
    }
    None
}

fn takes_value(flag: &str) -> bool {
    VALUE_FLAGS.contains(&flag)
}

fn is_subcommand(token: &OsStr) -> bool {
    token.to_str().is_some_and(|s| SUBCOMMANDS.contains(&s))
}

fn has_help_flag(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn insert_after(args: &[OsString], at: usize, word: &str) -> Vec<OsString> {
    let mut out = args.to_vec();
    out.insert(at + 1, OsString::from(word));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(args: &[&str]) -> Vec<String> {
        rewrite_agent_argv(args.iter().copied())
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn bare_agent_becomes_repl() {
        assert_eq!(
            rewrite(&["deslicer", "agent"]),
            ["deslicer", "agent", "repl"]
        );
    }

    #[test]
    fn flags_only_still_become_repl() {
        assert_eq!(
            rewrite(&["deslicer", "agent", "-a", "slicer", "--verbose"]),
            ["deslicer", "agent", "repl", "-a", "slicer", "--verbose"]
        );
    }

    #[test]
    fn a_bare_prompt_becomes_run() {
        assert_eq!(
            rewrite(&["deslicer", "agent", "Which indexers?"]),
            ["deslicer", "agent", "run", "Which indexers?"]
        );
    }

    #[test]
    fn a_prompt_after_agent_flags_becomes_run() {
        assert_eq!(
            rewrite(&["deslicer", "agent", "-a", "slicer", "hello"]),
            ["deslicer", "agent", "run", "-a", "slicer", "hello"]
        );
    }

    #[test]
    fn known_subcommands_are_left_alone() {
        for cmd in SUBCOMMANDS {
            let args = ["deslicer", "agent", cmd];
            assert_eq!(rewrite(&args), args, "{cmd}");
        }
    }

    #[test]
    fn help_flag_lists_subcommands_instead_of_entering_repl() {
        assert_eq!(
            rewrite(&["deslicer", "agent", "--help"]),
            ["deslicer", "agent", "--help"]
        );
        assert_eq!(
            rewrite(&["deslicer", "agent", "-h"]),
            ["deslicer", "agent", "-h"]
        );
    }

    #[test]
    fn global_flags_before_agent_are_preserved() {
        assert_eq!(
            rewrite(&["deslicer", "--log-format", "json", "agent"]),
            ["deslicer", "--log-format", "json", "agent", "repl"]
        );
    }

    #[test]
    fn other_commands_are_untouched() {
        assert_eq!(
            rewrite(&["deslicer", "auth", "login"]),
            ["deslicer", "auth", "login"]
        );
    }
}
