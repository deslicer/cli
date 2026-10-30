//! Shared `--limit` / `--cursor` pagination for list commands (#224).

use clap::Args as ClapArgs;
use serde::Serialize;
use serde_json::{json, Value};

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 500;

#[derive(Debug, Clone, ClapArgs)]
pub struct PageArgs {
    /// Max items to return (default 50, max 500).
    #[arg(long, default_value_t = DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..=MAX_LIMIT as i64))]
    pub limit: u32,

    /// Opaque continuation cursor from a previous page's `next_cursor`.
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    pub partial: bool,
}

impl<T: Serialize> Page<T> {
    pub fn from_items(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self {
            items,
            next_cursor,
            total: None,
            partial: false,
        }
    }

    pub fn to_json_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Slice a full in-memory list using an offset cursor (`"0"`, `"50"`, …).
pub fn page_slice<T: Clone + Serialize>(items: &[T], args: &PageArgs) -> Page<T> {
    let start = args
        .cursor
        .as_deref()
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0);
    let end = start.saturating_add(args.limit as usize);
    let page_items: Vec<T> = items
        .iter()
        .skip(start)
        .take(args.limit as usize)
        .cloned()
        .collect();
    let next_cursor = if end < items.len() {
        Some(end.to_string())
    } else {
        None
    };
    Page {
        items: page_items,
        next_cursor,
        total: Some(items.len() as u64),
        partial: false,
    }
}

pub fn print_page_json<T: Serialize>(page: &Page<T>) -> i32 {
    match page.to_json_value() {
        Ok(value) => crate::json_doc::print_success(&value),
        Err(err) => {
            eprintln!("failed to serialize page: {err}");
            1
        }
    }
}

/// Build query string fragments for Observer list endpoints.
pub fn query_pairs(args: &PageArgs, extra: &[(&str, Option<&str>)]) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();
    pairs.push(("limit".into(), args.limit.to_string()));
    if let Some(cursor) = args.cursor.as_deref() {
        pairs.push(("cursor".into(), cursor.to_string()));
    }
    for (key, value) in extra {
        if let Some(v) = value {
            pairs.push(((*key).into(), (*v).into()));
        }
    }
    if pairs.is_empty() {
        return String::new();
    }
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    format!("?{encoded}")
}

pub type ParsedListPayload = (Vec<Value>, Option<String>, Option<u64>);

/// Parse either a bare JSON array or `{items, next_cursor, total?}`.
pub fn parse_list_payload(bytes: &[u8]) -> Result<ParsedListPayload, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid list JSON: {e}"))?;
    match value {
        Value::Array(items) => Ok((items, None, None)),
        Value::Object(map) => {
            let items = map
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .ok_or_else(|| "list JSON object missing items[]".to_string())?;
            let next_cursor = map
                .get("next_cursor")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let total = map.get("total").and_then(|v| v.as_u64());
            Ok((items, next_cursor, total))
        }
        _ => Err("list JSON must be an array or {items, next_cursor}".into()),
    }
}

pub fn empty_page_json() -> Value {
    json!({
        "items": [],
        "next_cursor": Value::Null,
        "partial": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_slice_emits_offset_cursor() {
        let items: Vec<u32> = (0..120).collect();
        let page = page_slice(
            &items,
            &PageArgs {
                limit: 50,
                cursor: None,
            },
        );
        assert_eq!(page.items.len(), 50);
        assert_eq!(page.next_cursor.as_deref(), Some("50"));
        assert_eq!(page.total, Some(120));

        let page2 = page_slice(
            &items,
            &PageArgs {
                limit: 50,
                cursor: Some("50".into()),
            },
        );
        assert_eq!(page2.items[0], 50);
        assert_eq!(page2.next_cursor.as_deref(), Some("100"));
    }

    #[test]
    fn parse_array_and_object_shapes() {
        let (items, cursor, total) = parse_list_payload(br#"[{"id":"a"}]"#).unwrap();
        assert_eq!(items.len(), 1);
        assert!(cursor.is_none());
        assert!(total.is_none());

        let (items, cursor, total) =
            parse_list_payload(br#"{"items":[{"id":"a"}],"next_cursor":"n1","total":9}"#).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(cursor.as_deref(), Some("n1"));
        assert_eq!(total, Some(9));
    }
}
