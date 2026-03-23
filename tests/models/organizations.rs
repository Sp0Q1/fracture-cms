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
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-org-user-{suffix}"),
            email: format!("orguser-{suffix}@example.com"),
            name: Some(format!("Org User {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

#[tokio::test]
#[serial]
async fn test_personal_org_auto_created_on_user_creation() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "personal").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;

    assert_eq!(
        orgs.len(),
        1,
        "User should have exactly one org after creation"
    );
    assert!(orgs[0].is_personal, "First org should be personal");
    assert!(
        orgs[0].name.contains("Personal"),
        "Org name should contain 'Personal'"
    );

    // Verify user is owner
    let membership = org_members::Model::find_membership(db, orgs[0].id, user.id)
        .await
        .expect("User should be a member of their personal org");
    assert_eq!(membership.role, "owner");
}

#[tokio::test]
#[serial]
async fn test_find_by_pid() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "findpid").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
    let org = &orgs[0];

    let found = organizations::Model::find_by_pid(db, &org.pid.clone()).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, org.id);

    let not_found =
        organizations::Model::find_by_pid(db, "00000000-0000-0000-0000-000000000000").await;
    assert!(not_found.is_none());
}

#[tokio::test]
#[serial]
async fn test_find_by_slug() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "slug").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
    let org = &orgs[0];

    let found = organizations::Model::find_by_slug(db, &org.slug).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, org.id);
}

#[tokio::test]
#[serial]
async fn test_find_orgs_for_user() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "multiorg").await;

    // User has personal org from creation
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
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

    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
    assert_eq!(orgs.len(), 2);
}
