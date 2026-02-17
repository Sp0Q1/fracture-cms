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

#[async_trait]
impl Initializer for OidcInitializer {
    fn name(&self) -> String {
        "oidc".to_string()
    }

    async fn after_routes(&self, router: Router, ctx: &AppContext) -> Result<Router> {
        let Some(settings) = ctx.config.settings.as_ref() else {
            tracing::info!("No settings configured; OIDC disabled");
            return Ok(router);
        };

        let Some(oidc_value) = settings.get("oidc") else {
            tracing::info!("No OIDC settings found; OIDC disabled");
            return Ok(router);
        };

        let config: OidcConfig = serde_json::from_value(oidc_value.clone())
            .map_err(|e| loco_rs::Error::Message(format!("Failed to parse OIDC config: {e}")))?;

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

        let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
            .await
            .map_err(|e| loco_rs::Error::Message(format!("OIDC discovery failed: {e}")))?;

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
        };

        tracing::info!("OIDC initializer loaded successfully");
        Ok(router.layer(axum::Extension(oidc_ctx)))
    }
}
