use fracture_cms::{
    app::App,
    models::users::{self, OidcUserInfo},
};
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn orgs_list_returns_user_orgs_when_authenticated() {
    request::<App, _, _>(|request, ctx| async move {
        // Create a user (this also creates a personal org)
        let user = users::Model::find_or_create_from_oidc(
            &ctx.db,
            &OidcUserInfo {
                provider: "test".into(),
                subject: "org-request-test".into(),
                email: "orgreq@example.com".into(),
                name: Some("Org Req User".into()),
            },
        )
        .await
        .unwrap();

        // Generate a JWT
        let jwt = user
            .generate_jwt(&ctx.config.get_jwt_config().unwrap().secret, 3600)
            .unwrap();

        // Hit /orgs with auth cookie
        let response = request
            .get("/orgs")
            .add_cookie(axum_extra::extract::cookie::Cookie::new("jwt", jwt))
            .await;

        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(
            body.contains("Personal"),
            "Org list should contain personal org"
        );
    })
    .await;
}
