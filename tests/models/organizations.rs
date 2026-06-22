use fracture_cms::{
    app::App,
    models::{
        org_members::{self, OrgRole},
        organizations,
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use serial_test::serial;

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    let user = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-org-user-{suffix}"),
            email: format!("orguser-{suffix}@example.com"),
            name: Some(format!("Org User {suffix}")),
            email_verified: true,
        },
    )
    .await
    .expect("create test user");
    crate::support::owned_org(db, suffix, user.id).await;
    user
}

#[tokio::test]
#[serial]
async fn test_no_personal_org_and_default_org_join() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // A freshly created OIDC user gets NO org of their own (no personal orgs).
    let user = users::Model::find_or_create_from_oidc(
        db,
        &fracture_cms::models::users::OidcUserInfo {
            provider: "test".to_string(),
            subject: "default-org".to_string(),
            email: "default-org@example.com".to_string(),
            name: Some("Default Org".to_string()),
            email_verified: true,
        },
    )
    .await
    .unwrap();
    assert!(
        organizations::Model::find_orgs_for_user(db, user.id)
            .await
            .unwrap()
            .is_empty(),
        "no personal org should be auto-created"
    );

    // Joining the deployment's default org adds them as Viewer, idempotently.
    let org = organizations::Model::ensure_default_membership(db, "acme", "Acme Inc.", user.id)
        .await
        .unwrap();
    organizations::Model::ensure_default_membership(db, "acme", "Acme Inc.", user.id)
        .await
        .unwrap();
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    assert_eq!(orgs.len(), 1, "joined exactly the default org once");
    assert!(!orgs[0].is_personal);
    let membership = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .unwrap()
        .expect("default-org membership");
    assert_eq!(membership.role, "viewer", "default-org join is Viewer");
}

#[tokio::test]
#[serial]
async fn test_find_by_pid() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "findpid").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    let found = organizations::Model::find_by_pid(db, &org.pid.to_string())
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, org.id);

    let not_found = organizations::Model::find_by_pid(db, "00000000-0000-0000-0000-000000000000")
        .await
        .unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
#[serial]
async fn test_find_by_slug() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "slug").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    let found = organizations::Model::find_by_slug(db, &org.slug)
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, org.id);
}

#[tokio::test]
#[serial]
async fn test_find_orgs_for_user() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "multiorg").await;

    // create_test_user gives the user one owned org.
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    assert_eq!(orgs.len(), 1);

    // Create a second org
    let org2 = sea_orm::ActiveModelTrait::insert(
        fracture_cms::models::_entities::organizations::ActiveModel {
            name: sea_orm::ActiveValue::Set("Team Org".to_string()),
            slug: sea_orm::ActiveValue::Set(format!("team-org-{}", user.pid)),
            is_personal: sea_orm::ActiveValue::Set(false),
            ..Default::default()
        },
        db,
    )
    .await
    .unwrap();

    org_members::Model::add_member(db, org2.id, user.id, OrgRole::Member)
        .await
        .unwrap();

    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    assert_eq!(orgs.len(), 2);
}
