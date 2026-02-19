use fracture_cms::{
    app::App,
    models::{
        org_invites,
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
            subject: format!("test-invite-{suffix}"),
            email: format!("invite-{suffix}@example.com"),
            name: Some(format!("Invite User {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

#[tokio::test]
#[serial]
async fn test_create_and_find_invite() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "invowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    let invite = org_invites::Model::create_invite(
        db,
        org.id,
        "newuser@example.com",
        OrgRole::Member,
        owner.id,
    )
    .await
    .unwrap();

    assert_eq!(invite.email, "newuser@example.com");
    assert_eq!(invite.role, "member");
    assert!(invite.accepted_at.is_none());

    // Find by pid
    let found = org_invites::Model::find_by_pid(db, &invite.pid.to_string()).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, invite.id);
}

#[tokio::test]
#[serial]
async fn test_find_pending_by_email() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "pendowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    org_invites::Model::create_invite(db, org.id, "pending@example.com", OrgRole::Viewer, owner.id)
        .await
        .unwrap();

    let pending = org_invites::Model::find_pending_by_email(db, "pending@example.com").await;
    assert_eq!(pending.len(), 1);

    let none = org_invites::Model::find_pending_by_email(db, "nonexistent@example.com").await;
    assert!(none.is_empty());
}

#[tokio::test]
#[serial]
async fn test_accept_invite_creates_membership() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "accowner").await;
    let invitee = create_test_user(db, "accinvitee").await;

    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    let invite =
        org_invites::Model::create_invite(db, org.id, &invitee.email, OrgRole::Member, owner.id)
            .await
            .unwrap();

    org_invites::Model::accept_invite(db, invite, invitee.id)
        .await
        .unwrap();

    // Verify membership was created
    let membership = org_members::Model::find_membership(db, org.id, invitee.id).await;
    assert!(membership.is_some());
    assert_eq!(membership.unwrap().role, "member");
}

#[tokio::test]
#[serial]
async fn test_cannot_accept_already_accepted_invite() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "dblowner").await;
    let invitee = create_test_user(db, "dblinvitee").await;

    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    let invite =
        org_invites::Model::create_invite(db, org.id, &invitee.email, OrgRole::Member, owner.id)
            .await
            .unwrap();

    org_invites::Model::accept_invite(db, invite.clone(), invitee.id)
        .await
        .unwrap();

    // Reload invite to get the updated one
    let updated_invite = org_invites::Model::find_by_pid(db, &invite.pid.to_string())
        .await
        .unwrap();
    let result = org_invites::Model::accept_invite(db, updated_invite, invitee.id).await;
    assert!(result.is_err(), "Should not accept already-accepted invite");
}

#[tokio::test]
#[serial]
async fn test_auto_accept_on_oidc_signup() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // Create an org and invite an email that doesn't have an account yet
    let owner = create_test_user(db, "autoowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    org_invites::Model::create_invite(
        db,
        org.id,
        "newcomer-auto@example.com",
        OrgRole::Member,
        owner.id,
    )
    .await
    .unwrap();

    // Now create user via OIDC with that email - should auto-accept
    let newcomer = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: "test-newcomer-auto".to_string(),
            email: "newcomer-auto@example.com".to_string(),
            name: Some("Auto Newcomer".to_string()),
        },
    )
    .await
    .unwrap();

    // Check that they're a member of the org
    let membership = org_members::Model::find_membership(db, org.id, newcomer.id).await;
    assert!(
        membership.is_some(),
        "Invite should be auto-accepted on OIDC signup"
    );
    assert_eq!(membership.unwrap().role, "member");
}
