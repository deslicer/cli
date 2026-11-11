//! Commands named in `.agents/skills/deslicer-cli` must exist on the CLI. // pragma: allowlist secret

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use deslicer_cli::errors::PUBLISHED_EXIT_CODES; // pragma: allowlist secret
use deslicer_cli::schema::build_schema; // pragma: allowlist secret

const CLI_NAME: &str = "deslicer"; // pragma: allowlist secret // pragma: allowlist secret

#[test]
fn skill_commands_exist_in_the_cli_tree() {
    let markdown = skill_markdown();
    let commands = command_index();
    let mut resolved = BTreeSet::new();
    for invocation in invocations(&markdown) {
        let path = resolve_invocation(&invocation, &commands)
            .unwrap_or_else(|err| panic!("skill command `{invocation}`: {err}"));
        if let Some(path) = path {
            resolved.insert(path);
        }
    }
    for required in [
        "schema",
        "auth whoami",
        "auth login",
        "auth status",
        "init",
        "inventory validate",
        "groups list",
        "change plan",
        "change show",
        "change status",
        "change validate",
        "change approve",
        "change deploy",
        "change verify",
        "change revert",
    ] {
        assert!(
            resolved.contains(required),
            "skill never names `{required}`; resolved: {resolved:?}"
        );
    }
}

#[test]
fn skill_exit_code_table_matches_published_codes() {
    let markdown = skill_markdown();
    for code in PUBLISHED_EXIT_CODES {
        let row = format!("| {code} |");
        assert!(
            markdown.contains(&row),
            "skill exit-code table is missing {code}"
        );
    }
    for code in [9, 10, 22, 23, 24] {
        let line = table_line(&markdown, code);
        assert!(
            line.contains("| yes |"),
            "exit {code} should be retryable: {line}"
        );
    }
    for code in [1, 2, 5, 11, 12, 15, 20] {
        let line = table_line(&markdown, code);
        assert!(
            line.contains("| no |") || line.contains("| no* |"),
            "exit {code} should not be retryable: {line}"
        );
    }
}

#[test]
fn discovery_copies_match_the_canonical_skill() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let canonical = std::fs::read(skill_file(&root, ".agents/skills")).expect("canonical skill");
    for prefix in [".cursor/skills", ".claude/skills"] {
        let copy = std::fs::read(skill_file(&root, prefix))
            .unwrap_or_else(|err| panic!("read {prefix}: {err}"));
        assert_eq!(canonical, copy, "{prefix} drifted from .agents/skills");
    }
}

fn skill_markdown() -> String {
    let path = skill_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")), ".agents/skills");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn skill_file(root: &std::path::Path, skills_dir: &str) -> PathBuf {
    root.join(skills_dir)
        .join(format!("{CLI_NAME}-cli"))
        .join("SKILL.md")
}

fn table_line(markdown: &str, code: i32) -> String {
    let prefix = format!("| {code} |");
    markdown
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or("")
        .to_string()
}

/// path -> whether the command has subcommands
fn command_index() -> BTreeMap<String, bool> {
    let doc = build_schema(None).expect("schema");
    let mut index = BTreeMap::new();
    for command in doc["commands"].as_array().expect("commands") {
        let path = command["path"].as_str().expect("path").to_string();
        let has_subcommands = command["subcommands"]
            .as_array()
            .is_some_and(|subs| !subs.is_empty());
        index.insert(path, has_subcommands);
    }
    index
}

fn invocations(markdown: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = markdown;
    while let Some(rel) = rest.find(CLI_NAME) {
        let start = rel;
        let end = start + CLI_NAME.len();
        if invocation_boundary(rest, start) {
            let tail = &rest[end..];
            let stop = tail.find(['\n', '`']).unwrap_or(tail.len());
            found.push(tail[..stop].trim().to_string());
        }
        rest = &rest[end..];
    }
    found
}

fn invocation_boundary(text: &str, start: usize) -> bool {
    let prev = if start == 0 {
        None
    } else {
        text[..start].chars().next_back()
    };
    match prev {
        None => true,
        Some('`') => true,
        Some(ch) if ch.is_whitespace() => line_has_only_whitespace_before(text, start),
        _ => false,
    }
}

fn line_has_only_whitespace_before(text: &str, start: usize) -> bool {
    let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    text[line_start..start].chars().all(char::is_whitespace)
}

fn resolve_invocation(
    tail: &str,
    commands: &BTreeMap<String, bool>,
) -> Result<Option<String>, String> {
    let tokens = tokenize(strip_secret_allowlist(tail));
    let positionals = command_words(&tokens);
    if positionals.is_empty() {
        return Ok(None);
    }
    let mut path = Vec::new();
    let mut index = 0;
    while index < positionals.len() {
        let mut candidate = path.clone();
        candidate.push(positionals[index].clone());
        let key = candidate.join(" ");
        if commands.contains_key(&key) {
            path = candidate;
            index += 1;
            continue;
        }
        break;
    }
    if path.is_empty() {
        return Err(format!("unknown command {}", positionals[0]));
    }
    let key = path.join(" ");
    let has_subcommands = commands.get(&key).copied().unwrap_or(false);
    if index < positionals.len() && has_subcommands {
        return Err(format!(
            "`{key}` has no subcommand `{}`",
            positionals[index]
        ));
    }
    Ok(Some(key))
}

fn command_words(tokens: &[String]) -> Vec<String> {
    let mut index = skip_global_flags(tokens);
    let mut words = Vec::new();
    while index < tokens.len() {
        let token = &tokens[index];
        if token.starts_with('-') || token.starts_with('$') {
            break;
        }
        words.push(token.clone());
        index += 1;
    }
    words
}

fn skip_global_flags(tokens: &[String]) -> usize {
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if matches!(
            token,
            "--no-input" | "--non-interactive" | "--json" | "--help" | "-h" | "--version" | "-V"
        ) {
            index += 1;
            continue;
        }
        if matches!(
            token,
            "-o" | "--output"
                | "--log-format"
                | "--ci-platform"
                | "--deslicer-api-url" // pragma: allowlist secret
                | "--observer-api-url"
                | "--timeout"
                | "--retries"
        ) {
            index += 2;
            continue;
        }
        if token.starts_with("--output=")
            || token.starts_with("--log-format=")
            || token.starts_with("--deslicer-api-url=") // pragma: allowlist secret
            || token.starts_with("--observer-api-url=")
        {
            index += 1;
            continue;
        }
        break;
    }
    index
}

fn strip_secret_allowlist(tail: &str) -> &str {
    let markers = ["<!--", "# pragma: allowlist secret"];
    let mut end = tail.len();
    for marker in markers {
        if let Some(index) = tail.find(marker) {
            end = end.min(index);
        }
    }
    tail[..end].trim()
}

fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            } else {
                buf.push(ch);
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '#' => break,
            ch if ch.is_whitespace() => push_token(&mut tokens, &mut buf),
            _ => buf.push(ch),
        }
    }
    push_token(&mut tokens, &mut buf);
    tokens
}

fn push_token(tokens: &mut Vec<String>, buf: &mut String) {
    if !buf.is_empty() {
        tokens.push(std::mem::take(buf));
    }
}
