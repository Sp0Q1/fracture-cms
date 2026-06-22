use axum::{body::Body, response::Redirect, Extension};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use loco_rs::prelude::*;
use openidconnect::{
    core::CoreAuthenticationFlow, AuthorizationCode, EndUserEmail, EndUserName, LocalizedClaim,
    Nonce, PkceCodeChallenge, Scope, TokenResponse,
};
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    controllers::{
        middleware,
        oidc_state::{OidcContext, PendingAuth},
    },
    models::{_entities::users, users::OidcUserInfo},
};

/// Returns `true` when the configured server host uses HTTPS, meaning cookies
/// should be marked `Secure`.
fn is_secure(ctx: &AppContext) -> bool {
    ctx.config.server.host.starts_with("https")
}

/// Parses `settings.auth.allowed_email_domains` — the per-deployment login
/// admission allowlist. In a federated setup (one `IdP` shared by many tenant
/// deployments) this is the app-side trust boundary: without it, any user
/// the `IdP` authenticates gets an auto-provisioned account here.
///
/// Accepts a YAML list or a comma-separated string (env-var friendly).
/// `None` means no restriction is configured.
fn parse_allowed_domains(settings: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let value = settings?.get("auth")?.get("allowed_email_domains")?;
    let domains: Vec<String> = match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|i| i.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        serde_json::Value::String(s) => s
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => return None,
    };
    if domains.is_empty() {
        None
    } else {
        Some(domains)
    }
}

/// Returns true when `email`'s domain is in the allowlist (exact match,
/// case-insensitive).
fn email_domain_allowed(domains: &[String], email: &str) -> bool {
    let domain = email.rsplit('@').next().unwrap_or("").to_ascii_lowercase();
    !domain.is_empty() && domains.contains(&domain)
}

fn oidc_unavailable_response() -> Response {
    Response::builder()
        .status(503)
        .header("content-type", "text/html")
        .body(Body::from(
            "<h1>Authentication Not Available</h1>\
             <p>No identity provider has been configured. Contact the administrator.</p>",
        ))
        .unwrap()
        .into_response()
}

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

#[derive(Debug, Deserialize)]
struct BackchannelLogoutForm {
    logout_token: String,
}

#[debug_handler]
async fn authorize(oidc: Option<Extension<OidcContext>>) -> Result<Response> {
    let Some(Extension(oidc)) = oidc else {
        return Ok(oidc_unavailable_response());
    };
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
#[allow(clippy::too_many_lines)]
async fn callback(
    oidc: Option<Extension<OidcContext>>,
    State(ctx): State<AppContext>,
    Query(params): Query<CallbackParams>,
) -> Result<Response> {
    let Some(Extension(oidc)) = oidc else {
        return Ok(oidc_unavailable_response());
    };
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

    let expected_project_id = oidc.project_id.clone();
    let verifier = oidc
        .client
        .id_token_verifier()
        .set_other_audience_verifier_fn(move |aud| {
            // Zitadel includes the project ID in the aud claim alongside the client ID.
            let ok = aud.as_str() == expected_project_id;
            if !ok {
                eprintln!(
                    "[OIDC] Audience rejected: got={} expected={expected_project_id:?}",
                    aud.as_str()
                );
            }
            ok
        });
    let claims = id_token
        .claims(&verifier, &pending.nonce)
        .map_err(|e| loco_rs::Error::Message(format!("ID token verification failed: {e}")))?;

    let subject = claims.subject().to_string();
    let email = claims
        .email()
        .map(|e: &EndUserEmail| e.as_str().to_string())
        .ok_or_else(|| loco_rs::Error::Message("No email claim in ID token".to_string()))?;

    // Per-deployment admission boundary, checked on EVERY login (not just
    // signup) so removing a domain from the allowlist locks its users out.
    // The IdP authenticating an identity is not the same as this deployment
    // accepting it — see docs/FEDERATION.md.
    if let Some(domains) = parse_allowed_domains(ctx.config.settings.as_ref()) {
        if !email_domain_allowed(&domains, &email) {
            tracing::warn!(
                subject = %subject,
                "OIDC login rejected: email domain not in auth.allowed_email_domains"
            );
            return Err(loco_rs::Error::Unauthorized(
                "this identity is not permitted to access this application".to_string(),
            ));
        }
    }

    let name = claims
        .name()
        .and_then(|localized: &LocalizedClaim<EndUserName>| localized.get(None))
        .map(|n: &EndUserName| n.as_str().to_string());

    // Whether the `IdP` asserts this email as verified. This gates account
    // linking and invite auto-acceptance to prevent takeover via an
    // attacker-chosen email. The claim is optional: when absent it counts as
    // unverified unless the operator opted in via `assume_email_verified`
    // (for IdPs that only release verified emails but never emit the claim).
    let email_verified = claims
        .email_verified()
        .unwrap_or(oidc.assume_email_verified);

    let info = OidcUserInfo {
        provider: oidc.provider_name.clone(),
        subject,
        email,
        name,
        email_verified,
    };

    let user = users::Model::find_or_create_from_oidc(&ctx.db, &info).await?;

    // Place the user in the deployment's default org (one shared client org;
    // no per-user personal orgs). Configured via settings.org.default_slug /
    // default_name; the org is created on first use. Best-effort — a failure
    // here must not block an otherwise-valid login.
    if let Some(org_cfg) = ctx.config.settings.as_ref().and_then(|s| s.get("org")) {
        if let Some(slug) = org_cfg
            .get("default_slug")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            let name = org_cfg
                .get("default_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(slug);
            if let Err(e) = crate::models::organizations::Model::ensure_default_membership(
                &ctx.db, slug, name, user.id,
            )
            .await
            {
                tracing::warn!(error = %e, "could not ensure default-org membership");
            }
        }
    }

    let jwt_secret = ctx.config.get_jwt_config()?;
    let token = user
        .generate_jwt(&jwt_secret.secret, jwt_secret.expiration)
        .or_else(|_| unauthorized("unauthorized!"))?;

    let secure = is_secure(&ctx);

    let jwt_cookie = Cookie::build(("jwt", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();

    // Store the raw ID token for use as id_token_hint during logout.
    let id_token_cookie = Cookie::build(("id_token", id_token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();

    // Set org_pid cookie to the user's first org
    let orgs = crate::models::organizations::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    let org_pid_cookie = orgs.first().map(|org| {
        Cookie::build(("org_pid", org.pid.to_string()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(secure)
            .build()
    });

    let mut response = Redirect::temporary("/").into_response();
    let headers = response.headers_mut();
    headers.append(
        axum::http::header::SET_COOKIE,
        jwt_cookie
            .to_string()
            .parse()
            .expect("cookie is valid ASCII"),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        id_token_cookie
            .to_string()
            .parse()
            .expect("cookie is valid ASCII"),
    );
    if let Some(org_cookie) = org_pid_cookie {
        headers.append(
            axum::http::header::SET_COOKIE,
            org_cookie
                .to_string()
                .parse()
                .expect("cookie is valid ASCII"),
        );
    }
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
async fn logout(
    oidc: Option<Extension<OidcContext>>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let Some(Extension(oidc)) = oidc else {
        return Ok(Redirect::temporary("/").into_response());
    };
    let secure = is_secure(&ctx);
    let clear_jwt = Cookie::build(("jwt", ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::ZERO)
        .build();
    let clear_id_token = Cookie::build(("id_token", ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::ZERO)
        .build();

    // Redirect to the `IdP`'s end_session endpoint so the browser session is
    // terminated there as well.  Falls back to "/" if the `IdP` didn't advertise
    // an end_session_endpoint.
    let redirect_url = if let Some(end_session) = &oidc.end_session_url {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(hint) = jar.get("id_token") {
            params.push(("id_token_hint", hint.value().to_string()));
        }
        if !oidc.post_logout_redirect_uri.is_empty() {
            params.push((
                "post_logout_redirect_uri",
                oidc.post_logout_redirect_uri.clone(),
            ));
        }
        if params.is_empty() {
            end_session.clone()
        } else {
            let query = form_urlencoded::Serializer::new(String::new())
                .extend_pairs(params)
                .finish();
            format!("{end_session}?{query}")
        }
    } else {
        "/".to_string()
    };

    let mut response = Redirect::temporary(&redirect_url).into_response();
    let headers = response.headers_mut();
    headers.append(
        axum::http::header::SET_COOKIE,
        clear_jwt
            .to_string()
            .parse()
            .expect("cookie is valid ASCII"),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        clear_id_token
            .to_string()
            .parse()
            .expect("cookie is valid ASCII"),
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
        .secure(is_secure(&ctx))
        .build();
    let mut response = format::empty_json()?.into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().expect("cookie is valid ASCII"),
    );
    Ok(response)
}

/// Fetch the JWKS from the identity provider and find the decoding key matching the JWT
/// header's `kid`.
async fn fetch_decoding_key(
    jwks_uri: &str,
    header: &jsonwebtoken::Header,
) -> std::result::Result<jsonwebtoken::DecodingKey, loco_rs::Error> {
    let http_client = reqwest::Client::new();
    let resp = http_client
        .get(jwks_uri)
        .send()
        .await
        .map_err(|e| loco_rs::Error::Message(format!("Failed to fetch JWKS: {e}")))?;
    let body = resp
        .text()
        .await
        .map_err(|e| loco_rs::Error::Message(format!("Failed to read JWKS response: {e}")))?;
    let jwks: jsonwebtoken::jwk::JwkSet = serde_json::from_str(&body)
        .map_err(|e| loco_rs::Error::Message(format!("Failed to parse JWKS: {e}")))?;

    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| loco_rs::Error::Message("logout_token has no kid header".to_string()))?;

    let jwk = jwks
        .find(kid)
        .ok_or_else(|| loco_rs::Error::Message(format!("No JWKS key found for kid: {kid}")))?;

    jsonwebtoken::DecodingKey::from_jwk(jwk)
        .map_err(|e| loco_rs::Error::Message(format!("Failed to build key from JWK: {e}")))
}

#[debug_handler]
async fn backchannel_logout(
    oidc: Option<Extension<OidcContext>>,
    State(ctx): State<AppContext>,
    axum::Form(form): axum::Form<BackchannelLogoutForm>,
) -> Result<Response> {
    let Some(Extension(oidc)) = oidc else {
        return Ok((
            axum::http::StatusCode::NOT_IMPLEMENTED,
            "OIDC not configured",
        )
            .into_response());
    };
    let header = jsonwebtoken::decode_header(&form.logout_token)
        .map_err(|e| loco_rs::Error::Message(format!("Invalid logout_token header: {e}")))?;

    // Fetch the `IdP`'s signing key and verify the JWT signature
    let decoding_key = fetch_decoding_key(&oidc.jwks_uri, &header).await?;

    let mut validation = jsonwebtoken::Validation::new(header.alg);
    validation.set_issuer(&[&oidc.issuer_url]);
    validation.set_audience(&[&oidc.client_id]);
    // Zitadel may not include a `sub` in all cases, so don't require it
    validation.set_required_spec_claims(&["iss", "aud", "iat", "jti", "events"]);

    let token_data =
        jsonwebtoken::decode::<serde_json::Value>(&form.logout_token, &decoding_key, &validation)
            .map_err(|e| loco_rs::Error::Message(format!("logout_token validation failed: {e}")))?;

    let claims = &token_data.claims;

    // Verify the backchannel-logout event is present
    let has_event = claims
        .get("events")
        .and_then(|v| v.as_object())
        .is_some_and(|obj| obj.contains_key("http://schemas.openid.net/event/backchannel-logout"));
    if !has_event {
        return Ok((
            axum::http::StatusCode::BAD_REQUEST,
            "missing backchannel-logout event",
        )
            .into_response());
    }

    // Must not contain a nonce claim (per spec)
    if claims.get("nonce").is_some() {
        return Ok((
            axum::http::StatusCode::BAD_REQUEST,
            "logout_token must not contain nonce",
        )
            .into_response());
    }

    // Extract `sub` — if missing, we can't identify the user, but return 200 per spec
    let Some(sub) = claims.get("sub").and_then(|v| v.as_str()) else {
        return format::empty_json();
    };

    // Find user by OIDC subject + provider and invalidate their session
    if let Some(user) = users::Entity::find()
        .filter(
            model::query::condition()
                .eq(users::Column::OidcProvider, &oidc.provider_name)
                .eq(users::Column::OidcSubject, sub)
                .build(),
        )
        .one(&ctx.db)
        .await?
    {
        let mut active: users::ActiveModel = user.into();
        active.session_invalidated_at = ActiveValue::Set(Some(chrono::Utc::now().into()));
        active.update(&ctx.db).await?;
    }

    format::empty_json()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/auth/oidc")
        .add("/authorize", get(authorize))
        .add("/callback", get(callback))
        .add("/providers", get(providers))
        .add("/logout", get(logout))
        .add("/refresh", get(refresh))
        .add("/backchannel-logout", post(backchannel_logout))
}

#[cfg(test)]
mod tests {
    use super::{email_domain_allowed, parse_allowed_domains};
    use serde_json::json;

    #[test]
    fn no_settings_or_key_means_no_restriction() {
        assert!(parse_allowed_domains(None).is_none());
        assert!(parse_allowed_domains(Some(&json!({}))).is_none());
        assert!(parse_allowed_domains(Some(&json!({"auth": {}}))).is_none());
        // An empty list is treated as "not configured", not "deny everyone" —
        // a templated env var that resolves empty must not lock the app.
        let empty = json!({"auth": {"allowed_email_domains": []}});
        assert!(parse_allowed_domains(Some(&empty)).is_none());
        let blank = json!({"auth": {"allowed_email_domains": ""}});
        assert!(parse_allowed_domains(Some(&blank)).is_none());
    }

    #[test]
    fn parses_yaml_list_and_csv_string() {
        let list = json!({"auth": {"allowed_email_domains": ["Example.com", " corp.io "]}});
        assert_eq!(
            parse_allowed_domains(Some(&list)).unwrap(),
            vec!["example.com", "corp.io"]
        );
        let csv = json!({"auth": {"allowed_email_domains": "Example.com, corp.io ,"}});
        assert_eq!(
            parse_allowed_domains(Some(&csv)).unwrap(),
            vec!["example.com", "corp.io"]
        );
    }

    #[test]
    fn domain_matching_is_exact_and_case_insensitive() {
        let domains = vec!["example.com".to_string()];
        assert!(email_domain_allowed(&domains, "user@example.com"));
        assert!(email_domain_allowed(&domains, "user@EXAMPLE.COM"));
        // Subdomains and lookalikes are NOT allowed by an exact-domain list.
        assert!(!email_domain_allowed(&domains, "user@sub.example.com"));
        assert!(!email_domain_allowed(&domains, "user@evilexample.com"));
        assert!(!email_domain_allowed(&domains, "user@other.org"));
        // An email with a sneaky extra @ is judged by its real domain.
        assert!(email_domain_allowed(&domains, "a@b@example.com"));
        assert!(!email_domain_allowed(&domains, "user@example.com@evil.org"));
        assert!(!email_domain_allowed(&domains, "no-at-sign"));
    }
}
