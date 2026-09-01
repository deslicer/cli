use super::catalog::{hosted_slug, Topic, DOCS_BASE_URL_ENV, GITHUB_DOCS_BLOB_BASE};

/// Resolve the page URL for a topic.
///
/// Default: GitHub blob of `cli/docs` on `main` (works today).
/// If `DESLICER_DOCS_BASE_URL` is set and the topic is customer-facing,
/// use `{base}/{slug}` (Docusaurus strips `NN-` prefixes).
pub fn topic_url(topic: &Topic) -> String {
    topic_url_with_env(topic, std::env::var(DOCS_BASE_URL_ENV).ok().as_deref())
}

pub fn topic_url_with_env(topic: &Topic, hosted_base: Option<&str>) -> String {
    let hosted = hosted_base.map(str::trim).filter(|value| !value.is_empty());
    let mut url = if topic.sync {
        if let Some(base) = hosted {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                hosted_slug(topic.dest_file)
            )
        } else {
            format!(
                "{}/{}",
                GITHUB_DOCS_BLOB_BASE.trim_end_matches('/'),
                topic.source_file
            )
        }
    } else {
        format!(
            "{}/{}",
            GITHUB_DOCS_BLOB_BASE.trim_end_matches('/'),
            topic.source_file
        )
    };
    if let Some(anchor) = topic.anchor {
        url.push('#');
        url.push_str(anchor);
    }
    url
}

pub fn docs_base_for_list() -> String {
    std::env::var(DOCS_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| GITHUB_DOCS_BLOB_BASE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::docs::catalog::lookup;

    #[test]
    fn github_default_uses_source_path() {
        let topic = lookup("path-a2").expect("path-a2");
        let url = topic_url_with_env(topic, None);
        assert!(url.starts_with("https://github.com/deslicer/cli/blob/main/docs/quickstart.md#"));
        assert!(url.ends_with("path-a2-ci-pipeline-with-an-observer-api-token"));
    }

    #[test]
    fn hosted_base_uses_slug() {
        let topic = lookup("quickstart").expect("quickstart");
        let url = topic_url_with_env(topic, Some("https://docs.deslicer.io/cli/"));
        assert_eq!(url, "https://docs.deslicer.io/cli/quickstart");
    }

    #[test]
    fn internal_topics_stay_on_github() {
        let topic = lookup("contributing").expect("contributing");
        let url = topic_url_with_env(topic, Some("https://docs.deslicer.io/cli"));
        assert!(url.contains("github.com/deslicer/cli/blob/main/docs/contributing.md"));
    }
}
