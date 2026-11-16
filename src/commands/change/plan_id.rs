//! Shared `--plan-id` parser. Every change command that takes a plan id rejects
//! a non-UUID locally (exit 2) instead of sending it and mis-reading HTTP 400
//! as an unsupported CI platform (issue #263).

pub fn parse_plan_id(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    uuid::Uuid::parse_str(trimmed)
        .map(|_| trimmed.to_string())
        .map_err(|_| "plan id must be a UUID".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_uuid_and_accepts_uuid() {
        assert!(parse_plan_id("not-a-uuid").is_err());
        assert!(parse_plan_id("totally-not-a-plan").is_err());
        assert_eq!(
            parse_plan_id("87597ca8-ac36-409f-9b68-a4db995172f1").unwrap(),
            "87597ca8-ac36-409f-9b68-a4db995172f1"
        );
        assert_eq!(
            parse_plan_id("  87597ca8-ac36-409f-9b68-a4db995172f1  ").unwrap(),
            "87597ca8-ac36-409f-9b68-a4db995172f1"
        );
    }
}
