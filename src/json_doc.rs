//! Success-document JSON helpers: every document carries `schema_version` (#223).

use serde::Serialize;
use serde_json::{json, Value};

pub const SCHEMA_VERSION: u32 = 1;

/// Attach `schema_version` additively.
///
/// - Objects gain a `schema_version` field.
/// - Arrays become `{schema_version, items:[…]}` (arrays cannot carry fields).
/// - Other scalars become `{schema_version, value:…}`.
pub fn attach_schema_version(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            map.insert("schema_version".into(), json!(SCHEMA_VERSION));
            Value::Object(map)
        }
        Value::Array(items) => json!({
            "schema_version": SCHEMA_VERSION,
            "items": items,
        }),
        other => json!({
            "schema_version": SCHEMA_VERSION,
            "value": other,
        }),
    }
}

pub fn to_value(value: &(impl Serialize + ?Sized)) -> Result<Value, serde_json::Error> {
    Ok(attach_schema_version(serde_json::to_value(value)?))
}

pub fn to_pretty_string(value: &(impl Serialize + ?Sized)) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&to_value(value)?)
}

pub fn to_compact_string(value: &(impl Serialize + ?Sized)) -> Result<String, serde_json::Error> {
    serde_json::to_string(&to_value(value)?)
}

/// Print a success JSON document and return exit 0, or 1 on serialize failure.
pub fn print_success(value: &(impl Serialize + ?Sized)) -> i32 {
    match to_pretty_string(value) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(err) => {
            eprintln!("failed to serialize output: {err}");
            1
        }
    }
}

/// Compact one-line success JSON (CI / plan stream style).
pub fn print_success_compact(value: &(impl Serialize + ?Sized)) -> i32 {
    match to_compact_string(value) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(err) => {
            eprintln!("failed to serialize output: {err}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_gets_schema_version_field() {
        let out = attach_schema_version(json!({"plan_id": "p1", "status": "draft"}));
        assert_eq!(out["schema_version"], 1);
        assert_eq!(out["plan_id"], "p1");
    }

    #[test]
    fn array_wraps_as_items() {
        let out = attach_schema_version(json!([{"id": 1}, {"id": 2}]));
        assert_eq!(out["schema_version"], 1);
        assert_eq!(out["items"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn does_not_overwrite_existing_schema_version() {
        // Insert always sets the contract version; callers should not set their own.
        let out = attach_schema_version(json!({"schema_version": 99, "ok": true}));
        assert_eq!(out["schema_version"], 1);
        assert_eq!(out["ok"], true);
    }
}
