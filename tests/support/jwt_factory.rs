use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use deslicer_cli::ci::CiPlatform;
use serde_json::{json, Value};

const DUMMY_SIG: &str = "c2ltcGxlLXJzMjU2LXRlc3Qtc2ln";

fn base_claims(platform: CiPlatform) -> Value {
    match platform {
        CiPlatform::Github => json!({
            "iss": "https://token.actions.githubusercontent.com",
            "aud": "https://api.deslicer.ai",
            "repository": "acme/widgets",
            "repository_owner": "acme",
            "repository_id": "123",
            "ref": "refs/heads/main",
            "sha": "abc",
            "actor": "ci"
        }),
        CiPlatform::Gitlab => json!({
            "iss": "https://gitlab.com",
            "aud": "https://api.deslicer.ai",
            "project_path": "acme/widgets"
        }),
        CiPlatform::Azure => json!({
            "iss": "https://vstoken.dev.azure.com",
            "aud": "https://api.deslicer.ai",
            "repository": "acme/widgets",
            "definitionname": "main-pipeline"
        }),
        CiPlatform::Bitbucket => json!({
            "iss": "https://api.bitbucket.org/2.0/workspaces",
            "aud": "https://api.deslicer.ai",
            "repository": "acme/widgets",
            "workspaceUuid": "ws-uuid"
        }),
        CiPlatform::Local => json!({
            "iss": "local",
            "aud": "https://api.deslicer.ai"
        }),
    }
}

fn merge_claims(base: &mut Value, overrides: Value) {
    let Some(over) = overrides.as_object() else {
        return;
    };
    let Some(base_obj) = base.as_object_mut() else {
        return;
    };
    for (key, value) in over {
        base_obj.insert(key.clone(), value.clone());
    }
}

pub fn mint_jwt(platform: CiPlatform, claim_overrides: Value) -> String {
    let mut claims = base_claims(platform);
    merge_claims(&mut claims, claim_overrides);

    let header = json!({"alg": "RS256", "kid": "test-key"});
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());

    format!("{header_b64}.{payload_b64}.{DUMMY_SIG}")
}
