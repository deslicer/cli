use super::{OidcError, OidcTokenProvider};

pub struct GithubProvider;

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}

fn required_env(name: &str) -> Result<String, OidcError> {
    std::env::var(name).map_err(|_| OidcError::MissingEnv(name.to_string()))
}

#[derive(serde::Deserialize)]
struct GithubTokenResponse {
    value: String,
}

#[async_trait::async_trait]
impl OidcTokenProvider for GithubProvider {
    async fn fetch_token(&self, audience: &str) -> Result<String, OidcError> {
        let request_token = required_env("ACTIONS_ID_TOKEN_REQUEST_TOKEN")?;
        let request_url = required_env("ACTIONS_ID_TOKEN_REQUEST_URL")?;

        let url = format!(
            "{request_url}&audience={}",
            percent_encode_query(audience)
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {request_token}"))
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OidcError::Http(format!(
                "GitHub OIDC request failed with status {}",
                response.status()
            )));
        }

        let body = response
            .json::<GithubTokenResponse>()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        Ok(body.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{header, method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn fetch_token_returns_jwt_from_mock_server() {
        let _guard = ENV_LOCK.lock().unwrap();

        let server = MockServer::start().await;
        let audience = "https://api.deslicer.ai";

        Mock::given(method("GET"))
            .and(header(
                "Authorization",
                "Bearer dummy-request-token",
            ))
            .and(query_param("audience", audience))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": "github-jwt-token"
            })))
            .mount(&server)
            .await;

        std::env::set_var(
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            format!("{}?something=1", server.uri()),
        );
        std::env::set_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "dummy-request-token");

        let token = GithubProvider
            .fetch_token(audience)
            .await
            .unwrap();

        assert_eq!(token, "github-jwt-token");

        std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_URL");
        std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
    }

    #[tokio::test]
    async fn fetch_token_errors_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
        std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_URL");

        let err = GithubProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        assert!(matches!(err, OidcError::MissingEnv(_)));
    }
}
