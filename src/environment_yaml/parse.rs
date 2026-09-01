//! Text scan of an existing environment file for `apps:` blocks.
//!
//! Purely line-based so operator formatting and comments inside a list
//! survive byte-for-byte (same approach as Observer `yaml.rs`).

/// An operator-authored `apps:` block captured from the existing repo file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingAppsBlock {
    pub group_name: String,
    /// Text after `apps:` on the same line (e.g. ` []`), usually empty.
    pub inline_suffix: String,
    /// Raw lines under `apps:` (indent > 4), preserved verbatim.
    pub body_lines: Vec<String>,
}

impl ExistingAppsBlock {
    pub fn source_paths(&self) -> Vec<String> {
        let mut paths = source_paths_from_inline(&self.inline_suffix);
        for line in &self.body_lines {
            if let Some(path) = source_path_from_line(line) {
                paths.push(path);
            }
        }
        paths
    }
}

/// Scan an existing environment file for `  - inventory_group: <name>` entries
/// and capture each group's raw `apps:` block.
pub fn extract_apps_blocks(existing_content: &str) -> Vec<ExistingAppsBlock> {
    let mut blocks = Vec::new();
    let mut current_group: Option<String> = None;
    let mut lines = existing_content.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("  - inventory_group:") {
            current_group = parse_group_scalar(rest.trim());
            continue;
        }

        let Some(group_name) = current_group.clone() else {
            continue;
        };
        let Some(suffix) = line.strip_prefix("    apps:") else {
            continue;
        };

        let mut body_lines = Vec::new();
        while let Some(next) = lines.peek() {
            if belongs_to_apps_body(next) {
                if let Some(owned) = lines.next() {
                    body_lines.push(owned.to_string());
                }
            } else {
                break;
            }
        }
        blocks.push(ExistingAppsBlock {
            group_name,
            inline_suffix: suffix.to_string(),
            body_lines,
        });
        current_group = None;
    }

    blocks
}

/// Lines belonging to an `apps:` body are indented deeper than the `apps:` key
/// itself (4 spaces). Blank lines terminate the block to stay conservative.
fn belongs_to_apps_body(line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    let indent = line.len() - line.trim_start_matches(' ').len();
    indent > 4
}

/// Parse the scalar written by [`super::generate::format_scalar`]: plain token
/// or single-quoted with `''` escapes.
pub fn parse_group_scalar(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(inner) = trimmed
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Some(inner.replace("''", "'"));
    }
    Some(trimmed.to_string())
}

fn source_path_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("- ")
        .and_then(|item| item.strip_prefix("source_path:"))
        .or_else(|| {
            trimmed
                .find("source_path:")
                .map(|idx| &trimmed[idx + "source_path:".len()..])
        })?;
    parse_group_scalar(rest)
}

fn source_paths_from_inline(suffix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = suffix;
    while let Some(idx) = rest.find("source_path:") {
        rest = &rest[idx + "source_path:".len()..];
        let value = rest.split([',', '}']).next().unwrap_or(rest);
        if let Some(path) = parse_group_scalar(value) {
            paths.push(path);
        }
    }
    paths
}
