use fracture_cms::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn providers_returns_empty_when_oidc_not_configured() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/oidc/providers").await;

        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        let providers = body["providers"].as_array().unwrap();
        assert!(providers.is_empty());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn authorize_fails_when_oidc_not_configured() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/oidc/authorize").await;
        // Without the OidcContext Extension, axum returns 500
        assert_eq!(response.status_code(), 500);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn callback_fails_when_oidc_not_configured() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .get("/api/auth/oidc/callback?code=test&state=test")
            .await;
        // Without the OidcContext Extension, axum returns 500
        assert_eq!(response.status_code(), 500);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn callback_rejects_missing_query_params() {
    request::<App, _, _>(|request, _ctx| async move {
        // No query params at all
        let response = request.get("/api/auth/oidc/callback").await;
        assert!(
            response.status_code() == 400
                || response.status_code() == 422
                || response.status_code() == 500,
            "Expected error status for missing query params, got {}",
            response.status_code()
        );

        // Missing state param
        let response = request.get("/api/auth/oidc/callback?code=test").await;
        assert!(
            response.status_code() == 400
                || response.status_code() == 422
                || response.status_code() == 500,
            "Expected error status for missing state param, got {}",
            response.status_code()
        );

        // Missing code param
        let response = request.get("/api/auth/oidc/callback?state=test").await;
        assert!(
            response.status_code() == 400
                || response.status_code() == 422
                || response.status_code() == 500,
            "Expected error status for missing code param, got {}",
            response.status_code()
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn providers_response_structure_is_correct() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/oidc/providers").await;

        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();

        // Verify the response has the correct structure
        assert!(body.is_object(), "Response should be a JSON object");
        assert!(
            body.get("providers").is_some(),
            "Response should have a 'providers' field"
        );
        assert!(
            body["providers"].is_array(),
            "'providers' should be an array"
        );
    })
    .await;
}
