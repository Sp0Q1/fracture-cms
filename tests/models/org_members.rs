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
            subject: format!("test-member-{suffix}"),
            email: format!("member-{suffix}@example.com"),
            name: Some(format!("Member {suffix}")),
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
    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    // Add user2 as member
    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    let membership = org_members::Model::find_membership(db, org.id, user2.id)
        .await
        .unwrap();
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

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    let updated =
        org_members::Model::update_role(db, org.id, user2.id, OrgRole::Owner, OrgRole::Admin)
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
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    let result = org_members::Model::remove_member(db, org.id, user.id, OrgRole::Owner).await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::LastOwner(_))),
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

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    // Add user2 as owner too
    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Owner)
        .await
        .unwrap();

    // Now user1 can be removed (user2 is still owner)
    let result = org_members::Model::remove_member(db, org.id, user1.id, OrgRole::Owner).await;
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

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Viewer)
        .await
        .unwrap();

    let members = org_members::Model::find_members(db, org.id).await.unwrap();
    assert_eq!(members.len(), 2);
}

#[tokio::test]
#[serial]
async fn test_cannot_demote_last_owner() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "demoteowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    let membership = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(membership.role, "owner");

    // Demoting the only owner to admin should be rejected to prevent
    // an org from having zero owners.
    let result =
        org_members::Model::update_role(db, org.id, user.id, OrgRole::Owner, OrgRole::Admin).await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::LastOwner(_))),
        "update_role should prevent demoting the last owner"
    );
}

#[tokio::test]
#[serial]
async fn test_add_duplicate_member_fails() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "dup1").await;
    let user2 = create_test_user(db, "dup2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    // First add succeeds
    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    // Second add of the same user should fail (DB unique constraint)
    let result = org_members::Model::add_member(db, org.id, user2.id, OrgRole::Admin).await;
    assert!(
        result.is_err(),
        "Adding a duplicate member should fail at the DB level"
    );
}

#[tokio::test]
#[serial]
async fn test_remove_non_owner_member() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "rmnotown1").await;
    let user2 = create_test_user(db, "rmnotown2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();

    let result = org_members::Model::remove_member(db, org.id, user2.id, OrgRole::Admin).await;
    assert!(
        result.is_ok(),
        "Should be able to remove a non-owner member"
    );

    // Verify membership is gone
    let gone = org_members::Model::find_membership(db, org.id, user2.id)
        .await
        .unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
#[serial]
async fn test_admin_cannot_remove_owner() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "ceilrm1").await;
    let user2 = create_test_user(db, "ceilrm2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Owner)
        .await
        .unwrap();

    // An Admin actor must not be able to evict an Owner, even though two
    // owners exist (the last-owner guard alone would allow it).
    let result = org_members::Model::remove_member(db, org.id, user2.id, OrgRole::Admin).await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::Forbidden)),
        "an admin must not remove an owner"
    );
}

#[tokio::test]
#[serial]
async fn test_admin_cannot_grant_or_revoke_owner() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "ceilrole1").await;
    let user2 = create_test_user(db, "ceilrole2").await;
    let user3 = create_test_user(db, "ceilrole3").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    org_members::Model::add_member(db, org.id, user2.id, OrgRole::Member)
        .await
        .unwrap();
    org_members::Model::add_member(db, org.id, user3.id, OrgRole::Owner)
        .await
        .unwrap();

    // Granting a role above the actor's own rank is refused...
    let result =
        org_members::Model::update_role(db, org.id, user2.id, OrgRole::Admin, OrgRole::Owner).await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::Forbidden)),
        "an admin must not grant the owner role"
    );

    // ...and so is touching a member who outranks the actor.
    let result =
        org_members::Model::update_role(db, org.id, user3.id, OrgRole::Admin, OrgRole::Member)
            .await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::Forbidden)),
        "an admin must not demote an owner"
    );
}

#[tokio::test]
#[serial]
async fn test_update_role_missing_membership_is_not_found() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user1 = create_test_user(db, "missing1").await;
    let user2 = create_test_user(db, "missing2").await;

    let orgs = organizations::Model::find_orgs_for_user(db, user1.id)
        .await
        .unwrap();
    let org = &orgs[0];

    // user2 is not a member of user1's org.
    let result =
        org_members::Model::update_role(db, org.id, user2.id, OrgRole::Owner, OrgRole::Member)
            .await;
    assert!(
        matches!(result, Err(org_members::MemberWriteError::NotFound)),
        "updating a non-member must be NotFound"
    );
}
