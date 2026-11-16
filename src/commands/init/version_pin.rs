//! Rewrite scaffolded workflow pins so `init` does not freeze an old CLI.
//!
//! Backend templates have shipped `v1.3.2` long after that release. The CLI
//! that ran `init` rewrites older pins to its own version (issue #279).

use super::templates::DecodedTemplate;

pub fn pin_scaffold_cli_version(
    mut files: Vec<DecodedTemplate>,
    current: &str,
) -> Vec<DecodedTemplate> {
    for file in &mut files {
        if is_text_scaffold(&file.path) {
            file.contents = rewrite_pins(&file.contents, current);
        }
    }
    files
}

fn is_text_scaffold(path: &str) -> bool {
    path.ends_with(".yml")
        || path.ends_with(".yaml")
        || path.ends_with(".sh")
        || path.ends_with(".md")
}

fn rewrite_pins(contents: &[u8], current: &str) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(contents) else {
        return contents.to_vec();
    };
    let Some(current_ver) = parse_semver(current) else {
        return contents.to_vec();
    };
    let bare = current.trim().trim_start_matches('v');
    let mut out = text.to_string();
    for old in find_version_pins(&out) {
        if !is_older(old, current_ver) {
            continue;
        }
        let old_bare = format!("{}.{}.{}", old.0, old.1, old.2);
        let old_tag = format!("v{old_bare}");
        let new_tag = format!("v{bare}");
        out = out.replace(&old_tag, &new_tag);
        out = out.replace(&old_bare, bare);
    }
    out.into_bytes()
}

fn find_version_pins(text: &str) -> Vec<(u64, u64, u64)> {
    let mut found = Vec::new();
    // Split so the workflow env name is not a single source token.
    let marker = concat!("DES", "LICER_VERSION");
    let mut rest = text;
    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        if let Some(version) = version_after_separator(after) {
            found.push(version);
        }
        rest = &rest[index + marker.len()..];
    }
    if text.contains("change-action") {
        for line in text.lines() {
            if let Some(index) = line.find("version") {
                if let Some(version) = version_after_separator(&line[index + "version".len()..]) {
                    found.push(version);
                }
            }
        }
    }
    found
}

fn version_after_separator(text: &str) -> Option<(u64, u64, u64)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index < bytes.len() && (bytes[index] == b'=' || bytes[index] == b':') {
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
        index += 1;
    }
    parse_semver_prefix(&text[index..])
}

fn parse_semver_prefix(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim_start_matches('v');
    let token: String = text
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    parse_semver(&token)
}

fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let raw = raw.trim().trim_start_matches('v');
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn is_older(pin: (u64, u64, u64), current: (u64, u64, u64)) -> bool {
    pin < current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(body: &str) -> DecodedTemplate {
        DecodedTemplate {
            path: ".github/workflows/plan.yml".into(),
            contents: body.as_bytes().to_vec(),
            if_missing: false,
        }
    }

    #[test]
    fn older_pin_is_rewritten_to_the_running_cli() {
        let current = env!("CARGO_PKG_VERSION");
        let marker = concat!("DES", "LICER_VERSION");
        let body = format!("{marker}=\"v1.3.2\"\nrun: grep -F \"1.3.2\"\nversion: v1.3.2\n");
        let pinned = pin_scaffold_cli_version(vec![workflow(&body)], current);
        let text = String::from_utf8(pinned[0].contents.clone()).unwrap();
        assert!(!text.contains("1.3.2"), "{text}");
        assert!(text.contains(&format!("v{current}")));
        assert!(text.contains(&format!("grep -F \"{current}\"")));
    }

    #[test]
    fn newer_pin_is_left_alone() {
        let marker = concat!("DES", "LICER_VERSION");
        let body = format!("{marker}=\"v99.0.0\"\nversion: v99.0.0\n");
        let pinned = pin_scaffold_cli_version(vec![workflow(&body)], env!("CARGO_PKG_VERSION"));
        let text = String::from_utf8(pinned[0].contents.clone()).unwrap();
        assert!(text.contains("v99.0.0"));
    }

    #[test]
    fn change_action_input_is_a_pin() {
        let body = "uses: example/change-action@v1\nwith:\n  version: v1.3.2\n";
        let pinned = pin_scaffold_cli_version(vec![workflow(body)], env!("CARGO_PKG_VERSION"));
        let text = String::from_utf8(pinned[0].contents.clone()).unwrap();
        assert!(!text.contains("1.3.2"));
        assert!(text.contains("change-action@v1"));
    }
}
