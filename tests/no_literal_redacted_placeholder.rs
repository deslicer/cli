//! Guard against accidental commit-time scrubbing of the CLI name in user-facing text.

use std::path::{Path, PathBuf};

const LITERAL: &str = "[REDACTED]";

/// Paths under `src/` or `docs/` that legitimately contain the placeholder string.
const SRC_DOCS_ALLOWLIST: &[&str] = &["src/clone_token.rs"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn is_allowlisted(rel: &Path) -> bool {
    let normalized = rel.to_string_lossy().replace('\\', "/");
    SRC_DOCS_ALLOWLIST
        .iter()
        .any(|allowed| normalized == *allowed)
}

fn scan_tree(root: &Path, rel_base: &Path, hits: &mut Vec<(PathBuf, usize, String)>) {
    for entry in std::fs::read_dir(root).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let rel = rel_base.join(entry.file_name());
        if path.is_dir() {
            scan_tree(&path, &rel, hits);
            continue;
        }
        let rel_str = rel.to_string_lossy();
        if !(rel_str.ends_with(".rs") || rel_str.ends_with(".md")) {
            continue;
        }
        if is_allowlisted(&rel) {
            continue;
        }
        let contents = std::fs::read_to_string(&path).expect("read file");
        for (line_no, line) in contents.lines().enumerate() {
            if line.contains(LITERAL) {
                hits.push((rel.clone(), line_no + 1, line.trim().to_string()));
            }
        }
    }
}

#[test]
fn src_and_docs_have_no_literal_redacted_placeholder_outside_masking_modules() {
    let root = repo_root();
    let mut hits = Vec::new();
    for sub in ["src", "docs"] {
        let tree = root.join(sub);
        if tree.is_dir() {
            scan_tree(&tree, Path::new(sub), &mut hits);
        }
    }
    if !hits.is_empty() {
        let report = hits
            .iter()
            .map(|(path, line, text)| format!("  {}:{}: {}", path.display(), line, text))
            .collect::<Vec<_>>()
            .join("\n");
        panic!("found literal `{LITERAL}` outside allowlist ({SRC_DOCS_ALLOWLIST:?}):\n{report}");
    }
}
