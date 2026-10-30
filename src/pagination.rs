//! Shared `--limit` / `--cursor` pagination for list commands (#224).
//!
//! CLI cursors are opaque decimal offsets. Observer plan list understands
//! `limit` + `offset` and returns `X-Total-Count` — never a `cursor` query param.

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

/// Decode a CLI cursor into a non-negative offset. Empty/`None` → 0.
pub fn cursor_to_offset(cursor: Option<&str>) -> Result<u64, String> {
    match cursor {
        None => Ok(0),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(0);
            }
            trimmed.parse::<u64>().map_err(|_| {
                format!("invalid --cursor {raw:?}: expected a non-negative integer offset")
            })
        }
    }
}

pub fn offset_to_cursor(offset: u64) -> String {
    offset.to_string()
}

/// Build `next_cursor` from a server page: offset + returned count vs total.
pub fn next_offset_cursor(
    offset: u64,
    returned: usize,
    limit: u32,
    total: Option<u64>,
) -> Option<String> {
    let end = offset.saturating_add(returned as u64);
    if let Some(total) = total {
        if end < total {
            return Some(offset_to_cursor(end));
        }
        return None;
    }
    // No total: assume another page exists when the server filled the limit.
    if returned >= limit as usize {
        Some(offset_to_cursor(end))
    } else {
        None
    }
}

/// Slice a full in-memory list using an offset cursor (`"0"`, `"50"`, …).
pub fn page_slice<T: Clone + Serialize>(items: &[T], args: &PageArgs) -> Page<T> {
    let start = cursor_to_offset(args.cursor.as_deref()).unwrap_or(0) as usize;
    let end = start.saturating_add(args.limit as usize);
    let page_items: Vec<T> = items
        .iter()
        .skip(start)
        .take(args.limit as usize)
        .cloned()
        .collect();
    let next_cursor = if end < items.len() {
        Some(offset_to_cursor(end as u64))
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

/// Build query string fragments for Observer list endpoints (`limit` + `offset`).
pub fn query_pairs(args: &PageArgs, extra: &[(&str, Option<&str>)]) -> Result<String, String> {
    let offset = cursor_to_offset(args.cursor.as_deref())?;
    let mut pairs: Vec<(String, String)> = Vec::new();
    pairs.push(("limit".into(), args.limit.to_string()));
    pairs.push(("offset".into(), offset.to_string()));
    for (key, value) in extra {
        if let Some(v) = value {
            pairs.push(((*key).into(), (*v).into()));
        }
    }
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    Ok(format!("?{encoded}"))
}

/// Read `X-Total-Count` (case-insensitive) from response headers.
pub fn total_from_headers(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("x-total-count")
        .or_else(|| headers.get("X-Total-Count"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
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

/// Lightweight RFC3339 check for `--since` (exit 2 on failure).
pub fn parse_rfc3339_timestamp(raw: &str) -> Result<&str, String> {
    let s = raw.trim();
    // Minimum: YYYY-MM-DDTHH:MM:SSZ (20 chars).
    if s.len() < 20 {
        return Err(format!(
            "invalid --since {raw:?}: expected RFC3339 timestamp (e.g. 2026-01-15T00:00:00Z)"
        ));
    }
    let b = s.as_bytes();
    let date_ok = b[4] == b'-'
        && b[7] == b'-'
        && (b[10] == b'T' || b[10] == b't')
        && b[13] == b':'
        && b[16] == b':';
    if !date_ok {
        return Err(format!(
            "invalid --since {raw:?}: expected RFC3339 timestamp (e.g. 2026-01-15T00:00:00Z)"
        ));
    }
    let rest = &s[19..];
    let ok_suffix = rest.starts_with('Z')
        || rest.starts_with('z')
        || rest.starts_with('+')
        || rest.starts_with('-')
        || rest.starts_with('.');
    if !ok_suffix {
        return Err(format!(
            "invalid --since {raw:?}: expected RFC3339 timestamp (e.g. 2026-01-15T00:00:00Z)"
        ));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

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
    fn query_pairs_sends_offset_not_cursor() {
        let q = query_pairs(
            &PageArgs {
                limit: 50,
                cursor: Some("50".into()),
            },
            &[],
        )
        .unwrap();
        assert!(q.contains("limit=50"));
        assert!(q.contains("offset=50"));
        assert!(!q.contains("cursor="));
    }

    #[test]
    fn next_offset_uses_total_count() {
        assert_eq!(
            next_offset_cursor(0, 50, 50, Some(120)).as_deref(),
            Some("50")
        );
        assert_eq!(next_offset_cursor(100, 20, 50, Some(120)), None);
        assert_eq!(next_offset_cursor(0, 50, 50, None).as_deref(), Some("50"));
        assert_eq!(next_offset_cursor(0, 10, 50, None), None);
    }

    #[test]
    fn total_from_headers_reads_x_total_count() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Total-Count", HeaderValue::from_static("120"));
        assert_eq!(total_from_headers(&headers), Some(120));
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

    #[test]
    fn rejects_non_numeric_cursor() {
        assert!(cursor_to_offset(Some("abc")).is_err());
    }

    #[test]
    fn parses_rfc3339_since() {
        assert!(parse_rfc3339_timestamp("2026-01-15T00:00:00Z").is_ok());
        assert!(parse_rfc3339_timestamp("not-a-date").is_err());
    }
}
