use fracture_cms::{
    app::App,
    models::users::{self, OidcUserInfo},
};
use loco_rs::testing::prelude::*;
use sea_orm::{ConnectionTrait, Statement};
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

/// Reproduces the prod bug: UUID stored as text via raw SQL causes
/// SeaORM .all() to fail decoding. Tests that find_orgs_for_user
/// handles this correctly with the .one() workaround.
#[tokio::test]
#[serial]
async fn orgs_list_works_with_text_uuid_in_sqlite() {
    request::<App, _, _>(|request, ctx| async move {
        let user = users::Model::find_or_create_from_oidc(
            &ctx.db,
            &OidcUserInfo {
                provider: "test".into(),
                subject: "text-uuid-test".into(),
                email: "textuuid@example.com".into(),
                name: Some("Text UUID User".into()),
            },
        )
        .await
        .unwrap();

        // Insert an org with a TEXT UUID via raw SQL (like prod seed migration)
        ctx.db
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "INSERT INTO organizations (pid, name, slug, is_personal, created_at, updated_at) \
                     VALUES ('99999999-aaaa-bbbb-cccc-dddddddddddd', 'Text UUID Org', 'text-uuid-org', 0, \
                     datetime('now'), datetime('now'))"
                ),
            ))
            .await
            .unwrap();

        // Add user as member of the text-UUID org
        ctx.db
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "INSERT INTO org_members (org_id, user_id, role, created_at, updated_at) \
                     SELECT id, {}, 'owner', datetime('now'), datetime('now') \
                     FROM organizations WHERE slug = 'text-uuid-org'",
                    user.id
                ),
            ))
            .await
            .unwrap();

        let jwt = user
            .generate_jwt(&ctx.config.get_jwt_config().unwrap().secret, 3600)
            .unwrap();

        let response = request
            .get("/orgs")
            .add_cookie(axum_extra::extract::cookie::Cookie::new("jwt", jwt))
            .await;

        assert_eq!(response.status_code(), 200);
        let body = response.text();
        eprintln!("[TEST] body contains 'Text UUID Org': {}", body.contains("Text UUID Org"));
        assert!(
            body.contains("Text UUID Org"),
            "Org list should contain the text-UUID org"
        );
        assert!(
            body.contains("Personal"),
            "Org list should also contain personal org"
        );
    })
    .await;
}
