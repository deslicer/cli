//! Client-side `--filter` for list commands (#187).
//!
//! The needle is a case-insensitive substring. An absent or blank filter
//! returns the collection unchanged so default listings stay complete.

/// Keep items whose haystack contains `filter` (case-insensitive).
pub fn apply<T>(filter: Option<&str>, items: Vec<T>, haystack: impl Fn(&T) -> String) -> Vec<T> {
    let Some(needle) = filter.map(str::trim).filter(|s| !s.is_empty()) else {
        return items;
    };
    let needle = needle.to_ascii_lowercase();
    items
        .into_iter()
        .filter(|item| haystack(item).to_ascii_lowercase().contains(&needle))
        .collect()
}

/// True when `filter` matches any of the fields.
pub fn matches(filter: Option<&str>, fields: &[Option<&str>]) -> bool {
    let Some(needle) = filter.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let needle = needle.to_ascii_lowercase();
    fields
        .iter()
        .flatten()
        .any(|field| field.to_ascii_lowercase().contains(needle.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_filter_keeps_every_row() {
        let rows = vec!["alpha".to_string(), "beta".to_string()];
        let kept = apply(Some("  "), rows.clone(), |row| row.clone());
        assert_eq!(kept, rows);
        assert!(matches(None, &[Some("nope")]));
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let rows = vec!["Search Heads", "Indexers"];
        let kept = apply(Some("HEAD"), rows, |row| (*row).to_string());
        assert_eq!(kept, vec!["Search Heads"]);
        assert!(matches(Some("dex"), &[Some("Indexers"), None]));
        assert!(!matches(Some("dex"), &[Some("search")]));
    }
}
