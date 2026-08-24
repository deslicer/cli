use uuid::Uuid;

use crate::errors::CliError;

pub fn parse_jti(raw: &str) -> Result<Uuid, CliError> {
    Uuid::parse_str(raw.trim()).map_err(|_| {
        CliError::Other("--jti must be a UUID (the token id from `deslicer enroll list`)".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_uuid() {
        let id = parse_jti("019f36d6-3f61-7eea-9417-7ac4a8a10f69").expect("uuid");
        assert_eq!(id.to_string(), "019f36d6-3f61-7eea-9417-7ac4a8a10f69");
    }

    #[test]
    fn rejects_non_uuid() {
        let err = parse_jti("not-a-jti").expect_err("reject");
        assert!(err.to_string().contains("UUID"));
    }
}
