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
    provider_name: String,
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    post_logout_redirect_uri: String,
    #[serde(default = "default_scopes")]
    scopes: Vec<String>,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

pub struct OidcInitializer;

#[allow(clippy::too_many_lines)]
async fn setup_oidc(ctx: &AppContext, router: Router) -> Result<Router> {
    let Some(settings) = ctx.config.settings.as_ref() else {
        tracing::warn!("OIDC not configured — no settings section. Authentication disabled.");
        return Ok(router);
    };

    let Some(oidc_value) = settings.get("oidc") else {
        tracing::warn!("OIDC not configured — no oidc settings. Authentication disabled.");
        return Ok(router);
    };

    let Ok(config) = serde_json::from_value::<OidcConfig>(oidc_value.clone()) else {
        tracing::warn!("OIDC not configured — failed to parse config. Authentication disabled.");
        return Ok(router);
    };

    if config.client_id.is_empty() || config.issuer_url.is_empty() {
        tracing::warn!(
            "OIDC not configured — client_id or issuer_url is empty. Authentication disabled."
        );
        return Ok(router);
    }

    if config.client_secret.is_empty() {
        tracing::warn!("OIDC not configured — client_secret is empty. Authentication disabled.");
        return Ok(router);
    }

    tracing::info!(
        provider = %config.provider_name,
        issuer = %config.issuer_url,
        "Initializing OIDC provider"
    );

    let issuer_url = IssuerUrl::new(config.issuer_url.clone())
        .map_err(|e| loco_rs::Error::Message(format!("Invalid issuer URL: {e}")))?;

    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| loco_rs::Error::Message(format!("Failed to build HTTP client: {e}")))?;

    // discover_async fetches the discovery document AND pre-caches JWKS
    // keys (needed for ID token signature verification in the callback).
    let provider_metadata =
        match CoreProviderMetadata::discover_async(issuer_url, &http_client).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "OIDC discovery failed (IdP unreachable?): {e}. Authentication disabled."
                );
                return Ok(router);
            }
        };

    // Extract end_session_endpoint from the discovery document separately,
    // since CoreProviderMetadata uses EmptyAdditionalProviderMetadata and
    // doesn't expose it.  The IdP is guaranteed reachable at this point.
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        config.issuer_url.trim_end_matches('/')
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

    let client_id_str = config.client_id.clone();
    let issuer_url_str = config.issuer_url.clone();

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id),
        Some(ClientSecret::new(config.client_secret)),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.redirect_uri)
            .map_err(|e| loco_rs::Error::Message(format!("Invalid redirect URI: {e}")))?,
    );

    let oidc_ctx = OidcContext {
        client,
        state_store: OidcStateStore::new(),
        provider_name: config.provider_name,
        project_id: config.project_id,
        scopes: config.scopes,
        end_session_url,
        post_logout_redirect_uri: config.post_logout_redirect_uri,
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
        match setup_oidc(ctx, router.clone()).await {
            Ok(router) => Ok(router),
            Err(e) => {
                tracing::warn!("OIDC initialization failed: {e}. Authentication disabled.");
                Ok(router)
            }
        }
    }
}
