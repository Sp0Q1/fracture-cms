use axum::{response::Redirect, Extension};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use loco_rs::prelude::*;
use openidconnect::{
    core::CoreAuthenticationFlow, AuthorizationCode, EndUserEmail, EndUserName, LocalizedClaim,
    Nonce, PkceCodeChallenge, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    controllers::{
        middleware,
        oidc_state::{OidcContext, PendingAuth},
    },
    models::{_entities::users, users::OidcUserInfo},
};

#[derive(Debug, Serialize)]
struct ProviderInfo {
    name: String,
}

#[derive(Debug, Serialize)]
struct ProvidersResponse {
    providers: Vec<ProviderInfo>,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

#[debug_handler]
async fn authorize(Extension(oidc): Extension<OidcContext>) -> Result<Response> {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let nonce = Nonce::new_random();
    let nonce_clone = nonce.clone();

    let mut auth_request = oidc
        .client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            openidconnect::CsrfToken::new_random,
            move || nonce_clone,
        )
        .set_pkce_challenge(pkce_challenge);

    for scope in &oidc.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }

    let (authorize_url, csrf_token, _nonce) = auth_request.url();

    oidc.state_store.insert(
        &csrf_token,
        PendingAuth {
            pkce_verifier,
            nonce,
            created_at: Instant::now(),
        },
    );

    Ok(Redirect::temporary(authorize_url.as_str()).into_response())
}

#[debug_handler]
async fn callback(
    Extension(oidc): Extension<OidcContext>,
    State(ctx): State<AppContext>,
    Query(params): Query<CallbackParams>,
) -> Result<Response> {
    let pending = oidc
        .state_store
        .take(&params.state)
        .ok_or_else(|| loco_rs::Error::Unauthorized("Invalid or expired CSRF state".to_string()))?;

    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| loco_rs::Error::Message(format!("HTTP client error: {e}")))?;

    let token_response = oidc
        .client
        .exchange_code(AuthorizationCode::new(params.code))
        .map_err(|e| loco_rs::Error::Message(format!("Token endpoint not configured: {e}")))?
        .set_pkce_verifier(pending.pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| loco_rs::Error::Message(format!("Token exchange failed: {e}")))?;

    let id_token = token_response
        .id_token()
        .ok_or_else(|| loco_rs::Error::Message("No ID token in response".to_string()))?;

    let verifier = oidc.client.id_token_verifier();
    let claims = id_token
        .claims(&verifier, &pending.nonce)
        .map_err(|e| loco_rs::Error::Message(format!("ID token verification failed: {e}")))?;

    let subject = claims.subject().to_string();
    let email = claims
        .email()
        .map(|e: &EndUserEmail| e.as_str().to_string())
        .ok_or_else(|| loco_rs::Error::Message("No email claim in ID token".to_string()))?;

    let name = claims
        .name()
        .and_then(|localized: &LocalizedClaim<EndUserName>| localized.get(None))
        .map(|n: &EndUserName| n.as_str().to_string());

    let info = OidcUserInfo {
        provider: oidc.provider_name.clone(),
        subject,
        email,
        name,
    };

    let user = users::Model::find_or_create_from_oidc(&ctx.db, &info).await?;

    let jwt_secret = ctx.config.get_jwt_config()?;
    let token = user
        .generate_jwt(&jwt_secret.secret, jwt_secret.expiration)
        .or_else(|_| unauthorized("unauthorized!"))?;

    let cookie = Cookie::build(("jwt", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    let mut response = Redirect::temporary("/movies").into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );
    Ok(response)
}

#[debug_handler]
async fn providers(oidc: Option<Extension<OidcContext>>) -> Result<Response> {
    let providers = match oidc {
        Some(Extension(ctx)) => vec![ProviderInfo {
            name: ctx.provider_name,
        }],
        None => vec![],
    };
    format::json(ProvidersResponse { providers })
}

#[debug_handler]
async fn logout() -> Result<Response> {
    let cookie = Cookie::build(("jwt", ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build();
    let mut response = Redirect::temporary("/").into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );
    Ok(response)
}

#[debug_handler]
async fn refresh(State(ctx): State<AppContext>, jar: CookieJar) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx)
        .await
        .ok_or_else(|| loco_rs::Error::Unauthorized("not authenticated".into()))?;
    let jwt_config = ctx.config.get_jwt_config()?;
    let token = user
        .generate_jwt(&jwt_config.secret, jwt_config.expiration)
        .map_err(|_| loco_rs::Error::Unauthorized("token generation failed".into()))?;
    let cookie = Cookie::build(("jwt", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();
    let mut response = format::empty_json()?.into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );
    Ok(response)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/auth/oidc")
        .add("/authorize", get(authorize))
        .add("/callback", get(callback))
        .add("/providers", get(providers))
        .add("/logout", get(logout))
        .add("/refresh", get(refresh))
}
