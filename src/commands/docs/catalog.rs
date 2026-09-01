//! Topics for `deslicer docs` and the CLI → `deslicer/docs` sync map.
//!
//! Human guides live in `docs/*.md` (source of truth). This catalog is the
//! in-binary index: short titles, aliases, and whether a page is customer-facing.

/// GitHub blob root for `docs/*.md` on `main` (always reachable).
pub const GITHUB_DOCS_BLOB_BASE: &str = "https://github.com/deslicer/cli/blob/main";

/// Optional hosted docs root (`DESLICER_DOCS_BASE_URL`), e.g. `https://docs.deslicer.io/cli`.
pub const DOCS_BASE_URL_ENV: &str = "DESLICER_DOCS_BASE_URL";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Topic {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub title: &'static str,
    pub summary: &'static str,
    /// Path from the CLI repo root (`docs/quickstart.md`).
    pub source_file: &'static str,
    /// Destination under `products/cli/` when `sync` is true.
    pub dest_file: &'static str,
    pub anchor: Option<&'static str>,
    /// Customer-facing: included in `deslicer/docs` sync.
    pub sync: bool,
}

pub const TOPICS: &[Topic] = &[
    Topic {
        id: "quickstart",
        aliases: &[],
        title: "Quickstart",
        summary: "Path A (OIDC), Path A2 (Observer token), and Path B (bundle)",
        source_file: "docs/quickstart.md",
        dest_file: "01-quickstart.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "path-a2",
        aliases: &["a2", "github-token"],
        title: "Path A2 (GitHub token)",
        summary: "Git-sourced plans with DESLICER_API_TOKEN (no GitHub App / OIDC)",
        source_file: "docs/quickstart.md",
        dest_file: "01-quickstart.md",
        anchor: Some("path-a2-ci-pipeline-with-an-observer-api-token"),
        sync: true,
    },
    Topic {
        id: "init",
        aliases: &["repo-init", "enroll"],
        title: "Repo init and enroll",
        summary: "deslicer init, enrollment tokens, and worker install",
        source_file: "docs/repo-init-and-enroll.md",
        dest_file: "02-repo-init-and-enroll.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "install",
        aliases: &["installation"],
        title: "Installation",
        summary: "Homebrew, cargo, curl, CI runners, and updates",
        source_file: "docs/installation.md",
        dest_file: "03-installation.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "architecture",
        aliases: &[],
        title: "Architecture",
        summary: "How the CLI talks to DAI and Observer",
        source_file: "docs/architecture.md",
        dest_file: "04-architecture.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "bundle",
        aliases: &["path-b"],
        title: "Bundle flow (Path B)",
        summary: "change plan --source-dir when Observer cannot clone the repo",
        source_file: "docs/bundle-flow.md",
        dest_file: "05-bundle-flow.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "ci-outputs",
        aliases: &[],
        title: "CI outputs",
        summary: "plan_id and other GitHub Actions / GitLab outputs",
        source_file: "docs/ci-outputs.md",
        dest_file: "06-ci-outputs.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "environments",
        aliases: &[],
        title: "Environments",
        summary: "Portal environment bindings and --environment",
        source_file: "docs/environments.md",
        dest_file: "07-environments.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "oidc",
        aliases: &["oidc-troubleshooting"],
        title: "OIDC troubleshooting",
        summary: "CI OIDC exchange failures",
        source_file: "docs/oidc-troubleshooting.md",
        dest_file: "08-oidc-troubleshooting.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "agent",
        aliases: &["agent-runs"],
        title: "Agent runs",
        summary: "deslicer agent REPL and one-shot runs",
        source_file: "docs/agent-runs.md",
        dest_file: "09-agent-runs.md",
        anchor: None,
        sync: true,
    },
    Topic {
        id: "local-testing",
        aliases: &[],
        title: "Local testing",
        summary: "Laptop checkout against a local Observer (internal)",
        source_file: "docs/local-testing.md",
        dest_file: "local-testing.md",
        anchor: None,
        sync: false,
    },
    Topic {
        id: "contributing",
        aliases: &[],
        title: "Contributing",
        summary: "CLI contributor guide (internal)",
        source_file: "docs/contributing.md",
        dest_file: "contributing.md",
        anchor: None,
        sync: false,
    },
    Topic {
        id: "release",
        aliases: &["release-process"],
        title: "Release process",
        summary: "How CLI releases are cut (internal)",
        source_file: "docs/release-process.md",
        dest_file: "release-process.md",
        anchor: None,
        sync: false,
    },
];

pub fn lookup(name: &str) -> Option<&'static Topic> {
    let key = name.trim().to_ascii_lowercase();
    TOPICS
        .iter()
        .find(|topic| topic.id == key || topic.aliases.iter().any(|alias| *alias == key))
}

pub fn known_topic_ids() -> Vec<&'static str> {
    TOPICS.iter().map(|topic| topic.id).collect()
}

/// Strip `NN-` prefix and `.md` for a hosted `/cli/<slug>` URL.
pub fn hosted_slug(dest_file: &str) -> String {
    let stem = dest_file
        .strip_suffix(".md")
        .unwrap_or(dest_file)
        .rsplit('/')
        .next()
        .unwrap_or(dest_file);
    let bytes = stem.as_bytes();
    if bytes.len() >= 4
        && bytes[2] == b'-'
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
    {
        stem[3..].to_string()
    } else {
        stem.to_string()
    }
}

/// Customer-facing pages, unique by destination file (aliases share a dest).
#[cfg(test)]
fn sync_pages() -> Vec<&'static Topic> {
    let mut pages: Vec<&'static Topic> = Vec::new();
    for topic in TOPICS {
        if topic.sync
            && pages
                .iter()
                .all(|existing| existing.dest_file != topic.dest_file)
        {
            pages.push(topic);
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_aliases_path_a2() {
        assert_eq!(lookup("path-a2").map(|t| t.id), Some("path-a2"));
        assert_eq!(lookup("github-token").map(|t| t.id), Some("path-a2"));
        assert_eq!(lookup("INIT").map(|t| t.id), Some("init"));
        assert!(lookup("nope").is_none());
    }

    #[test]
    fn hosted_slug_strips_number_prefix() {
        assert_eq!(hosted_slug("01-quickstart.md"), "quickstart");
        assert_eq!(hosted_slug("local-testing.md"), "local-testing");
    }

    #[test]
    fn topic_ids_are_unique() {
        let mut ids: Vec<&str> = known_topic_ids();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TOPICS.len());
    }

    #[test]
    fn every_source_file_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for topic in TOPICS {
            let path = root.join(topic.source_file);
            assert!(
                path.is_file(),
                "missing {} for topic {}",
                path.display(),
                topic.id
            );
        }
    }

    #[test]
    fn sync_pages_dedupes_path_a2_onto_quickstart() {
        let dests: Vec<&str> = sync_pages().iter().map(|t| t.dest_file).collect();
        assert!(dests.contains(&"01-quickstart.md"));
        assert_eq!(
            dests.iter().filter(|d| **d == "01-quickstart.md").count(),
            1
        );
        assert!(!dests.iter().any(|d| d.contains("local-testing")));
    }

    #[derive(serde::Deserialize)]
    struct SyncManifest {
        page: Vec<ManifestPage>,
    }

    #[derive(serde::Deserialize)]
    struct ManifestPage {
        id: String,
        source: String,
        dest: String,
    }

    #[test]
    fn manifest_matches_catalog_sync_pages() {
        let raw = include_str!("../../../docs/sync-manifest.toml");
        let manifest: SyncManifest = toml::from_str(raw).expect("sync-manifest.toml");
        let pages = sync_pages();
        assert_eq!(manifest.page.len(), pages.len());
        for (page, topic) in manifest.page.iter().zip(pages.iter()) {
            assert_eq!(page.id, topic.id);
            assert_eq!(page.source, topic.source_file);
            assert_eq!(page.dest, topic.dest_file);
        }
    }
}
