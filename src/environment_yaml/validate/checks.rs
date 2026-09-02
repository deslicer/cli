//! Destination / app field checks for environment YAML validation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{issue, Severity, ValidationIssue, VALID_DEST_DIRS, VALID_STATES};

pub(super) struct ValidationCtx<'a> {
    pub file_label: &'a str,
    pub project_root: &'a Path,
    pub known_groups: Option<&'a HashSet<String>>,
    pub seen_groups: &'a mut HashMap<String, usize>,
    pub issues: &'a mut Vec<ValidationIssue>,
}

pub(super) fn validate_destination(
    destination: &serde_yml::Value,
    idx: usize,
    ctx: &mut ValidationCtx<'_>,
) {
    let base = format!("destinations[{idx}]");
    let Some(map) = destination.as_mapping() else {
        ctx.issues.push(issue(
            ctx.file_label,
            &base,
            Severity::Error,
            "each destination must be a mapping".into(),
            "Format each destination as `- inventory_group: <name>`",
        ));
        return;
    };

    let inventory_group = match scalar_string(map.get("inventory_group")) {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        Some(_) => {
            ctx.issues.push(issue(
                ctx.file_label,
                &format!("{base}.inventory_group"),
                Severity::Error,
                "inventory_group must be a non-empty string".into(),
                "Set inventory_group to a host-group name from `deslicer groups list`",
            ));
            String::new()
        }
        None => {
            ctx.issues.push(issue(
                ctx.file_label,
                &base,
                Severity::Error,
                "missing required field: inventory_group".into(),
                "Add `inventory_group: <host-group-name>` to this destination",
            ));
            String::new()
        }
    };

    if !inventory_group.is_empty() {
        check_duplicate_group(&inventory_group, idx, &base, ctx);
        check_live_group(&inventory_group, &base, ctx);
    }

    if map.contains_key("apps") {
        if let Some(apps) = map.get("apps") {
            validate_apps(apps, &base, ctx);
        }
    } else {
        validate_common_fields(map, &base, ctx);
        warn_if_not_splunk_app_root(&base, ctx);
    }
}

fn check_duplicate_group(
    inventory_group: &str,
    idx: usize,
    base: &str,
    ctx: &mut ValidationCtx<'_>,
) {
    if let Some(first_idx) = ctx.seen_groups.get(inventory_group) {
        ctx.issues.push(issue(
            ctx.file_label,
            &format!("{base}.inventory_group"),
            Severity::Error,
            format!(
                "duplicate inventory_group \"{inventory_group}\" (also at destinations[{first_idx}])"
            ),
            format!("Merge all apps for \"{inventory_group}\" into a single destination block"),
        ));
    } else {
        ctx.seen_groups.insert(inventory_group.to_string(), idx);
    }
}

fn check_live_group(inventory_group: &str, base: &str, ctx: &mut ValidationCtx<'_>) {
    let Some(known) = ctx.known_groups else {
        return;
    };
    if known.contains(inventory_group) {
        return;
    }
    let suggestion = if known.is_empty() {
        "No host groups returned from Observer. Create groups in the portal, then re-run."
            .to_string()
    } else {
        let mut names: Vec<&String> = known.iter().collect();
        names.sort();
        let preview = names
            .iter()
            .take(12)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let more = if names.len() > 12 { ", …" } else { "" };
        format!(
            "Use an exact host-group `name` from `deslicer groups list` \
             (known: {preview}{more})"
        )
    };
    ctx.issues.push(issue(
        ctx.file_label,
        &format!("{base}.inventory_group"),
        Severity::Error,
        format!("unknown inventory_group \"{inventory_group}\""),
        suggestion,
    ));
}

fn validate_apps(apps: &serde_yml::Value, base: &str, ctx: &mut ValidationCtx<'_>) {
    let apps_path = format!("{base}.apps");
    let Some(list) = apps.as_sequence() else {
        ctx.issues.push(issue(
            ctx.file_label,
            &apps_path,
            Severity::Error,
            "apps must be a list".into(),
            "Format apps as:\napps:\n  - source_path: apps/my_app",
        ));
        return;
    };

    let mut seen_apps: HashMap<String, usize> = HashMap::new();
    for (idx, app) in list.iter().enumerate() {
        let app_path = format!("{apps_path}[{idx}]");
        let Some(map) = app.as_mapping() else {
            ctx.issues.push(issue(
                ctx.file_label,
                &app_path,
                Severity::Error,
                "each app must be a mapping".into(),
                "Format each app as `- source_path: apps/my_app`",
            ));
            continue;
        };
        validate_app_entry(map, &app_path, &apps_path, idx, &mut seen_apps, ctx);
    }
}

fn validate_app_entry(
    map: &serde_yml::Mapping,
    app_path: &str,
    apps_path: &str,
    idx: usize,
    seen_apps: &mut HashMap<String, usize>,
    ctx: &mut ValidationCtx<'_>,
) {
    let source_path = scalar_string(map.get("source_path")).map(|value| value.trim().to_string());
    let dest_dir = scalar_string(map.get("dest_dir"))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "apps".to_string());
    let state = scalar_string(map.get("state"))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "present".to_string());

    match source_path.as_deref() {
        None => ctx.issues.push(issue(
            ctx.file_label,
            app_path,
            Severity::Error,
            "missing required field: source_path".into(),
            "Add `source_path: <relative-app-path>` to this app entry",
        )),
        Some("") => ctx.issues.push(issue(
            ctx.file_label,
            &format!("{app_path}.source_path"),
            Severity::Error,
            "source_path must be a non-empty string".into(),
            "Set source_path to a relative path under the repository root",
        )),
        Some(path) => {
            check_duplicate_app(path, &dest_dir, app_path, apps_path, idx, seen_apps, ctx);
            check_source_path(path, &state, app_path, ctx);
        }
    }

    validate_common_fields(map, app_path, ctx);
}

fn check_duplicate_app(
    path: &str,
    dest_dir: &str,
    app_path: &str,
    apps_path: &str,
    idx: usize,
    seen_apps: &mut HashMap<String, usize>,
    ctx: &mut ValidationCtx<'_>,
) {
    let key = format!("{path}:{dest_dir}");
    if let Some(first_idx) = seen_apps.get(&key) {
        ctx.issues.push(issue(
            ctx.file_label,
            app_path,
            Severity::Error,
            format!(
                "duplicate app \"{path}\" with dest_dir \"{dest_dir}\" \
                 (also at {apps_path}[{first_idx}])"
            ),
            "Remove the duplicate entry or change dest_dir if targeting a different location",
        ));
    } else {
        seen_apps.insert(key, idx);
    }
}

fn check_source_path(path: &str, state: &str, app_path: &str, ctx: &mut ValidationCtx<'_>) {
    if Path::new(path).is_absolute() {
        ctx.issues.push(issue(
            ctx.file_label,
            &format!("{app_path}.source_path"),
            Severity::Error,
            format!("source_path \"{path}\" should be relative, not absolute"),
            "Use a path from the repository root (e.g. apps/my_app)",
        ));
    }
    if path.contains("..") {
        ctx.issues.push(issue(
            ctx.file_label,
            &format!("{app_path}.source_path"),
            Severity::Warning,
            format!("source_path \"{path}\" contains \"..\""),
            "Prefer paths that stay within the repository",
        ));
    }
    if state != "absent" && !ctx.project_root.join(path).exists() {
        ctx.issues.push(issue(
            ctx.file_label,
            &format!("{app_path}.source_path"),
            Severity::Error,
            format!("source_path \"{path}\" does not exist on disk"),
            format!(
                "Create `{path}` under the repository root, or set `state: absent` \
                 if the app is being removed"
            ),
        ));
    }
}

fn validate_common_fields(map: &serde_yml::Mapping, obj_path: &str, ctx: &mut ValidationCtx<'_>) {
    if let Some(state) = map.get("state") {
        match scalar_string(Some(state)) {
            Some(value) if VALID_STATES.contains(&value.as_str()) => {}
            Some(value) => ctx.issues.push(issue(
                ctx.file_label,
                &format!("{obj_path}.state"),
                Severity::Error,
                format!("invalid state value: {value}"),
                format!("state must be one of: {}", VALID_STATES.join(", ")),
            )),
            None => ctx.issues.push(issue(
                ctx.file_label,
                &format!("{obj_path}.state"),
                Severity::Error,
                "state must be a string".into(),
                format!("state must be one of: {}", VALID_STATES.join(", ")),
            )),
        }
    }

    if let Some(dest_dir) = map.get("dest_dir") {
        match scalar_string(Some(dest_dir)) {
            Some(value) if VALID_DEST_DIRS.contains(&value.as_str()) => {}
            Some(value) => ctx.issues.push(issue(
                ctx.file_label,
                &format!("{obj_path}.dest_dir"),
                Severity::Error,
                format!("invalid dest_dir value: {value}"),
                format!("dest_dir must be one of: {}", VALID_DEST_DIRS.join(", ")),
            )),
            None => ctx.issues.push(issue(
                ctx.file_label,
                &format!("{obj_path}.dest_dir"),
                Severity::Error,
                "dest_dir must be a string".into(),
                format!("dest_dir must be one of: {}", VALID_DEST_DIRS.join(", ")),
            )),
        }
    }
}

fn warn_if_not_splunk_app_root(base: &str, ctx: &mut ValidationCtx<'_>) {
    let default_dir = ctx.project_root.join("default");
    let local_dir = ctx.project_root.join("local");
    if !default_dir.exists() && !local_dir.exists() {
        ctx.issues.push(issue(
            ctx.file_label,
            base,
            Severity::Warning,
            "mono-app destination has no apps list and repository root has no default/ or local/"
                .into(),
            "Add an `apps:` list with `source_path` entries for multi-app repos, \
             or place Splunk app content at the repository root",
        ));
    }
}

fn scalar_string(value: Option<&serde_yml::Value>) -> Option<String> {
    let value = value?;
    match value {
        serde_yml::Value::String(text) => Some(text.clone()),
        serde_yml::Value::Bool(flag) => Some(flag.to_string()),
        serde_yml::Value::Number(num) => Some(num.to_string()),
        serde_yml::Value::Tagged(tagged) => scalar_string(Some(tagged.value())),
        _ => None,
    }
}
