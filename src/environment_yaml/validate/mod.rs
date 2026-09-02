//! Fail-closed validation for `.deslicer/environments/*.yml`.
//!
//! CCA-style structural checks adapted to the DAP tenant env schema, plus an
//! optional live host-group allowlist from Observer (`GET /api/v1/groups`).

mod checks;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use checks::{validate_destination, ValidationCtx};

/// Allowed `state` values on destinations / apps.
pub const VALID_STATES: &[&str] = &["present", "absent"];

/// Allowed `dest_dir` values (CCA / DAP metadata spec).
pub const VALID_DEST_DIRS: &[&str] = &["apps", "shcluster/apps", "manager-apps", "deployment-apps"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    pub file: String,
    pub path: String,
    pub severity: Severity,
    pub message: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub file: String,
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
    }

    pub fn is_ok(&self) -> bool {
        self.errors().next().is_none()
    }
}

/// Validate one environment YAML document.
///
/// When `known_groups` is `Some`, every `inventory_group` must appear in that
/// set (exact name match against Observer host groups). Pass `None` only for
/// structural-only unit tests — the CLI always supplies a live set.
pub fn validate_environment_yaml(
    content: &str,
    file_label: &str,
    project_root: &Path,
    known_groups: Option<&HashSet<String>>,
) -> ValidationReport {
    let mut report = ValidationReport {
        file: file_label.to_string(),
        issues: Vec::new(),
    };

    let parsed: serde_yml::Value = match serde_yml::from_str(content) {
        Ok(value) => value,
        Err(err) => {
            report.issues.push(issue(
                file_label,
                "root",
                Severity::Error,
                format!("invalid YAML syntax: {err}"),
                "Fix YAML syntax errors",
            ));
            return report;
        }
    };

    let Some(root) = parsed.as_mapping() else {
        report.issues.push(issue(
            file_label,
            "root",
            Severity::Error,
            "configuration must be a YAML mapping".into(),
            "Use a mapping with a top-level `destinations:` list",
        ));
        return report;
    };

    let Some(destinations_value) = root.get("destinations") else {
        report.issues.push(issue(
            file_label,
            "root",
            Severity::Error,
            "missing required field: destinations".into(),
            "Add `destinations:` at the root with at least one inventory_group entry",
        ));
        return report;
    };

    let Some(destinations) = destinations_value.as_sequence() else {
        report.issues.push(issue(
            file_label,
            "destinations",
            Severity::Error,
            "destinations must be a list".into(),
            "Change destinations to a list:\ndestinations:\n  - inventory_group: example_group",
        ));
        return report;
    };

    if destinations.is_empty() {
        report.issues.push(issue(
            file_label,
            "destinations",
            Severity::Error,
            "destinations list is empty".into(),
            "Add at least one destination with an inventory_group",
        ));
        return report;
    }

    let mut seen_groups: HashMap<String, usize> = HashMap::new();
    let mut ctx = ValidationCtx {
        file_label,
        project_root,
        known_groups,
        seen_groups: &mut seen_groups,
        issues: &mut report.issues,
    };
    for (idx, destination) in destinations.iter().enumerate() {
        validate_destination(destination, idx, &mut ctx);
    }

    report
}

pub(super) fn issue(
    file: &str,
    path: &str,
    severity: Severity,
    message: String,
    suggestion: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        file: file.to_string(),
        path: path.to_string(),
        severity,
        message,
        suggestion: suggestion.into(),
    }
}

/// Resolve which env YAML file to validate under `dir`.
pub fn resolve_env_file(dir: &Path, stem: &str) -> Result<PathBuf, String> {
    let yml = dir
        .join(super::DESLICER_ENVIRONMENTS_DIR)
        .join(format!("{stem}.yml"));
    if yml.is_file() {
        return Ok(yml);
    }
    let yaml = dir
        .join(super::DESLICER_ENVIRONMENTS_DIR)
        .join(format!("{stem}.yaml"));
    if yaml.is_file() {
        return Ok(yaml);
    }
    Err(format!(
        "environment file not found: {} (or .yaml)",
        yml.display()
    ))
}

#[cfg(test)]
mod tests;
