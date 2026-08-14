use super::{OidcError, OidcTokenProvider};

pub struct AzureProvider;

fn required_env(name: &str) -> Result<String, OidcError> {
    std::env::var(name).map_err(|_| OidcError::MissingEnv(name.to_string()))
}

#[derive(serde::Deserialize)]
struct AzureTokenResponse {
    #[serde(rename = "oidcToken")]
    oidc_token: String,
}

#[async_trait::async_trait]
impl OidcTokenProvider for AzureProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        let request_uri = required_env("SYSTEM_OIDCREQUESTURI")?;
        let access_token = required_env("SYSTEM_ACCESSTOKEN")?;

        // Phase 1: POST to SYSTEM_OIDCREQUESTURI directly. Service-connection
        // query params (api-version, serviceConnectionId) are deferred.
        let parsed = url::Url::parse(&request_uri).map_err(|e| OidcError::Http(e.to_string()))?;
        crate::http::assert_url_allowed(&parsed).map_err(|e| OidcError::Http(e.to_string()))?;

        let client = crate::http::client();
        let response = client
            .post(&request_uri)
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OidcError::Http(format!(
                "Azure OIDC request failed with status {}",
                response.status()
            )));
        }

        let body = response
            .json::<AzureTokenResponse>()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        Ok(body.oidc_token)
    }
}

#[cfg(test)]
// ENV_LOCK only serializes env access across single-threaded #[tokio::test] cases;
// holding it across the await is safe (no cross-task contention).
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn fetch_token_returns_jwt_from_mock_server() {
        let _guard = ENV_LOCK.lock().unwrap();

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(header("Authorization", "Bearer dummy-access-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "oidcToken": "azure-jwt-token"
            })))
            .mount(&server)
            .await;

        std::env::set_var("SYSTEM_OIDCREQUESTURI", server.uri());
        std::env::set_var("SYSTEM_ACCESSTOKEN", "dummy-access-token");

        let token = AzureProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap();

        assert_eq!(token, "azure-jwt-token");

        std::env::remove_var("SYSTEM_OIDCREQUESTURI");
        std::env::remove_var("SYSTEM_ACCESSTOKEN");
    }

    #[tokio::test]
    async fn fetch_token_errors_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("SYSTEM_OIDCREQUESTURI");
        std::env::remove_var("SYSTEM_ACCESSTOKEN");

        let err = AzureProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        assert!(matches!(err, OidcError::MissingEnv(_)));
    }
}
