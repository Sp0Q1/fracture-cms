use async_trait::async_trait;
use axum::Router;
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata},
    ClientId, ClientSecret, IssuerUrl, RedirectUrl,
};
use serde::Deserialize;

use crate::controllers::oidc_state::{OidcContext, OidcStateStore};

#[derive(Debug, Deserialize)]
struct OidcConfig {
    #[serde(default)]
    provider_name: Option<String>,
    #[serde(default)]
    issuer_url: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    post_logout_redirect_uri: Option<String>,
    #[serde(default = "default_scopes")]
    scopes: Vec<String>,
    /// When `true`, missing/invalid OIDC config fails app boot with an
    /// error. When `false` or omitted (default), the initializer logs and
    /// silently disables auth — useful for `cargo loco doctor` style local
    /// inspection but **never set this in production**.
    ///
    /// Production deployments should set this to `true` in their YAML so a
    /// misconfigured `IdP` does not silently leave the app open / auth-less.
    #[serde(default)]
    required: bool,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

pub struct OidcInitializer;

/// Either skip OIDC setup (returning the router unchanged) or fail boot,
/// depending on the `required` flag. Centralised so every "missing config"
/// path applies the same rule.
fn skip_or_fail(required: bool, router: Router, reason: &str) -> Result<Router> {
    if required {
        // Production mode: refuse to boot with OIDC misconfigured. A
        // silently auth-less app is worse than a hard fail.
        Err(loco_rs::Error::Message(format!(
            "OIDC required by config but not usable: {reason}"
        )))
    } else {
        // Dev / inspection mode: log and proceed without auth.
        tracing::warn!("OIDC initializer: {reason}; authentication disabled");
        Ok(router)
    }
}

#[allow(clippy::too_many_lines)]
async fn setup_oidc(ctx: &AppContext, router: Router) -> Result<Router> {
    // First decide whether OIDC is required. We need the config to know,
    // so resolve `required` defensively.
    let required = ctx
        .config
        .settings
        .as_ref()
        .and_then(|s| s.get("oidc"))
        .and_then(|v| v.get("required"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let Some(settings) = ctx.config.settings.as_ref() else {
        return skip_or_fail(required, router, "no settings section in config");
    };

    let Some(oidc_value) = settings.get("oidc") else {
        return skip_or_fail(required, router, "no oidc settings section in config");
    };

    let Ok(config) = serde_json::from_value::<OidcConfig>(oidc_value.clone()) else {
        return skip_or_fail(required, router, "failed to parse oidc config block");
    };

    let client_id = config.client_id.unwrap_or_default();
    let issuer_url = config.issuer_url.unwrap_or_default();
    let client_secret = config.client_secret.unwrap_or_default();
    let redirect_uri = config.redirect_uri.unwrap_or_default();
    let provider_name = config.provider_name.unwrap_or_default();
    let post_logout_redirect_uri = config.post_logout_redirect_uri.unwrap_or_default();

    if client_id.is_empty() || issuer_url.is_empty() {
        return skip_or_fail(config.required, router, "client_id or issuer_url is empty");
    }

    if client_secret.is_empty() {
        return skip_or_fail(config.required, router, "client_secret is empty");
    }

    tracing::info!(provider = %provider_name, issuer = %issuer_url, "OIDC provider initializing");

    let issuer_url_parsed = IssuerUrl::new(issuer_url.clone())
        .map_err(|e| loco_rs::Error::Message(format!("Invalid issuer URL: {e}")))?;

    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| loco_rs::Error::Message(format!("Failed to build HTTP client: {e}")))?;

    // discover_async fetches the discovery document AND pre-caches JWKS
    // keys (needed for ID token signature verification in the callback).
    let provider_metadata =
        match CoreProviderMetadata::discover_async(issuer_url_parsed, &http_client).await {
            Ok(m) => m,
            Err(e) => {
                return skip_or_fail(config.required, router, &format!("discovery failed: {e}"));
            }
        };

    // Extract end_session_endpoint from the discovery document separately,
    // since CoreProviderMetadata uses EmptyAdditionalProviderMetadata and
    // doesn't expose it.  The IdP is guaranteed reachable at this point.
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );
    let (end_session_url, jwks_uri) = if let Ok(resp) = http_client.get(&discovery_url).send().await
    {
        let (mut end_session, mut jwks) = (None, None);
        if let Some(doc) = resp
            .text()
            .await
            .ok()
            .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        {
            end_session = doc
                .get("end_session_endpoint")
                .and_then(|v| v.as_str())
                .map(String::from);
            jwks = doc
                .get("jwks_uri")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        (end_session, jwks)
    } else {
        (None, None)
    };

    let jwks_uri = jwks_uri.ok_or_else(|| {
        loco_rs::Error::Message("OIDC discovery did not return jwks_uri".to_string())
    })?;

    tracing::info!(
        end_session = ?end_session_url,
        "OIDC end_session_endpoint discovery"
    );

    let client_id_str = client_id.clone();
    let issuer_url_str = issuer_url.clone();

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri)
            .map_err(|e| loco_rs::Error::Message(format!("Invalid redirect URI: {e}")))?,
    );

    let oidc_ctx = OidcContext {
        client,
        state_store: OidcStateStore::new(),
        provider_name,
        project_id: config.project_id.unwrap_or_default(),
        scopes: config.scopes,
        end_session_url,
        post_logout_redirect_uri,
        issuer_url: issuer_url_str,
        client_id: client_id_str,
        jwks_uri,
    };

    tracing::info!("OIDC initializer loaded successfully");
    Ok(router.layer(axum::Extension(oidc_ctx)))
}

#[async_trait]
impl Initializer for OidcInitializer {
    fn name(&self) -> String {
        "oidc".to_string()
    }

    async fn after_routes(&self, router: Router, ctx: &AppContext) -> Result<Router> {
        // Re-resolve `required` here so we can decide whether an unexpected
        // panic / Err from setup_oidc is fatal. setup_oidc itself uses its
        // own resolution; this is defence-in-depth for paths that bubble up
        // unexpected errors (e.g. invalid issuer URL formatting).
        let required = ctx
            .config
            .settings
            .as_ref()
            .and_then(|s| s.get("oidc"))
            .and_then(|v| v.get("required"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        match setup_oidc(ctx, router.clone()).await {
            Ok(router) => Ok(router),
            Err(e) => {
                if required {
                    Err(e)
                } else {
                    tracing::warn!(error = %e, "OIDC initialization failed; authentication disabled");
                    Ok(router)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_or_fail_returns_err_when_required() {
        let router = Router::new();
        let result = skip_or_fail(true, router, "no client_id");
        assert!(result.is_err(), "required=true must reject missing config");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("OIDC required"));
        assert!(msg.contains("no client_id"));
    }

    #[test]
    fn skip_or_fail_returns_ok_when_not_required() {
        let router = Router::new();
        let result = skip_or_fail(false, router, "no client_id");
        assert!(
            result.is_ok(),
            "required=false must allow boot without auth"
        );
    }

    #[test]
    fn oidc_config_required_defaults_to_false() {
        // Empty oidc block must parse with required=false so existing
        // dev configs without the new field continue to work.
        let json = serde_json::json!({});
        let cfg: OidcConfig = serde_json::from_value(json).unwrap();
        assert!(!cfg.required);
    }

    #[test]
    fn oidc_config_required_parses_true() {
        let json = serde_json::json!({ "required": true });
        let cfg: OidcConfig = serde_json::from_value(json).unwrap();
        assert!(cfg.required);
    }
}
