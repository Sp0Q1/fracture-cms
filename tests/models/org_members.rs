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
            subject: format!("test-member-{suffix}"),
            email: format!("member-{suffix}@example.com"),
            name: Some(format!("Member {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

#[tokio::test]
#[serial]
async fn test_role_hierarchy_at_least() {
    assert!(OrgRole::Owner.at_least(OrgRole::Owner));
    assert!(OrgRole::Owner.at_least(OrgRole::Admin));
    assert!(OrgRole::Owner.at_least(OrgRole::Member));
    assert!(OrgRole::Owner.at_least(OrgRole::Viewer));

    assert!(!OrgRole::Admin.at_least(OrgRole::Owner));
    assert!(OrgRole::Admin.at_least(OrgRole::Admin));
    assert!(OrgRole::Admin.at_least(OrgRole::Member));
    assert!(OrgRole::Admin.at_least(OrgRole::Viewer));

    assert!(!OrgRole::Member.at_least(OrgRole::Owner));
    assert!(!OrgRole::Member.at_least(OrgRole::Admin));
    assert!(OrgRole::Member.at_least(OrgRole::Member));
    assert!(OrgRole::Member.at_least(OrgRole::Viewer));

    assert!(!OrgRole::Viewer.at_least(OrgRole::Owner));
    assert!(!OrgRole::Viewer.at_least(OrgRole::Admin));
    assert!(!OrgRole::Viewer.at_least(OrgRole::Member));
    assert!(OrgRole::Viewer.at_least(OrgRole::Viewer));
}

#[tokio::test]
#[serial]
async fn test_role_from_str() {
    assert_eq!(OrgRole::from_str_role("owner"), Some(OrgRole::Owner));
    assert_eq!(OrgRole::from_str_role("admin"), Some(OrgRole::Admin));
    assert_eq!(OrgRole::from_str_role("member"), Some(OrgRole::Member));
    assert_eq!(OrgRole::from_str_role("viewer"), Some(OrgRole::Viewer));
    assert_eq!(OrgRole::from_str_role("invalid"), None);
}

#[tokio::test]
#[serial]
async fn test_add_and_find_membership() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "add1").await;
    let user2 = create_test_user(db, "add2").await;

    // user1 has a personal org from creation
    let orgs = organizations::Model::find_orgs_for_user(db, user1.id).await;
    let org = &orgs[0];

    // Add user2 as member
    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    let membership = org_members::Model::find_membership(db, org.id, user2.id).await;
    assert!(membership.is_some());
    assert_eq!(membership.unwrap().role, "member");
}

#[tokio::test]
#[serial]
async fn test_update_role() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "updrole1").await;
    let user2 = create_test_user(db, "updrole2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id).await;
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    let membership = org_members::Model::find_membership(db, org.id, user2.id)
        .await
        .unwrap();
    let updated = org_members::Model::update_role(db, membership, OrgRole::Admin)
        .await
        .unwrap();
    assert_eq!(updated.role, "admin");
}

#[tokio::test]
#[serial]
async fn test_cannot_remove_last_owner() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "lastowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
    let org = &orgs[0];

    let membership = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .unwrap();

    let result = org_members::Model::remove_member(db, membership).await;
    assert!(
        result.is_err(),
        "Should not be able to remove the last owner"
    );
}

#[tokio::test]
#[serial]
async fn test_can_remove_non_last_owner() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "rmowner1").await;
    let user2 = create_test_user(db, "rmowner2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id).await;
    let org = &orgs[0];

    // Add user2 as owner too
    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Owner)
        .await
        .unwrap();

    // Now user1 can be removed (user2 is still owner)
    let membership = org_members::Model::find_membership(db, org.id, user1.id)
        .await
        .unwrap();
    let result = org_members::Model::remove_member(db, membership).await;
    assert!(
        result.is_ok(),
        "Should be able to remove owner when another owner exists"
    );
}

#[tokio::test]
#[serial]
async fn test_find_members() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "fm1").await;
    let user2 = create_test_user(db, "fm2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id).await;
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Viewer)
        .await
        .unwrap();

    let members = org_members::Model::find_members(db, org.id).await;
    assert_eq!(members.len(), 2);
}
