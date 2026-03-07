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

#[tokio::test]
#[serial]
async fn test_expired_invite_cannot_be_accepted() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "expowner").await;
    let invitee = create_test_user(db, "expinvitee").await;

    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    let invite =
        org_invites::Model::create_invite(db, org.id, &invitee.email, OrgRole::Member, owner.id)
            .await
            .unwrap();

    // Manually set expires_at to the past to simulate an expired invite
    let expired_time = chrono::Utc::now() - chrono::Duration::days(1);
    let mut active: org_invites::ActiveModel = invite.into();
    active.expires_at = sea_orm::ActiveValue::Set(expired_time.into());
    let expired_invite = sea_orm::ActiveModelTrait::update(active, db).await.unwrap();

    let result = org_invites::Model::accept_invite(db, expired_invite, invitee.id).await;
    assert!(result.is_err(), "Should not accept an expired invite");
}

#[tokio::test]
#[serial]
async fn test_find_pending_by_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "orgpendowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    org_invites::Model::create_invite(
        db,
        org.id,
        "orgpend1@example.com",
        OrgRole::Member,
        owner.id,
    )
    .await
    .unwrap();
    org_invites::Model::create_invite(
        db,
        org.id,
        "orgpend2@example.com",
        OrgRole::Viewer,
        owner.id,
    )
    .await
    .unwrap();

    let pending = org_invites::Model::find_pending_by_org(db, org.id).await;
    assert_eq!(pending.len(), 2);

    // A different org should have no pending invites
    let other_user = create_test_user(db, "orgpendother").await;
    let other_orgs = organizations::Model::find_orgs_for_user(db, other_user.id).await;
    let other_pending = org_invites::Model::find_pending_by_org(db, other_orgs[0].id).await;
    assert!(other_pending.is_empty());
}

#[tokio::test]
#[serial]
async fn test_expired_invites_excluded_from_pending() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "expfiltowner").await;
    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    let invite = org_invites::Model::create_invite(
        db,
        org.id,
        "expfilt@example.com",
        OrgRole::Member,
        owner.id,
    )
    .await
    .unwrap();

    // Expire the invite
    let expired_time = chrono::Utc::now() - chrono::Duration::days(1);
    let mut active: org_invites::ActiveModel = invite.into();
    active.expires_at = sea_orm::ActiveValue::Set(expired_time.into());
    sea_orm::ActiveModelTrait::update(active, db).await.unwrap();

    // Should not appear in pending queries
    let pending_by_email =
        org_invites::Model::find_pending_by_email(db, "expfilt@example.com").await;
    assert!(
        pending_by_email.is_empty(),
        "Expired invites should not appear in pending-by-email"
    );

    let pending_by_org = org_invites::Model::find_pending_by_org(db, org.id).await;
    assert!(
        pending_by_org.is_empty(),
        "Expired invites should not appear in pending-by-org"
    );
}

#[tokio::test]
#[serial]
async fn test_accept_invite_idempotent_for_existing_member() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = create_test_user(db, "idemowner").await;
    let member = create_test_user(db, "idemmember").await;

    let orgs = organizations::Model::find_orgs_for_user(db, owner.id).await;
    let org = &orgs[0];

    // Add member first
    org_members::Model::add_member(db, org.id, member.id, OrgRole::Viewer)
        .await
        .unwrap();

    // Create invite for the already-existing member
    let invite =
        org_invites::Model::create_invite(db, org.id, &member.email, OrgRole::Admin, owner.id)
            .await
            .unwrap();

    // Accept should succeed without duplicating the membership
    org_invites::Model::accept_invite(db, invite, member.id)
        .await
        .unwrap();

    // Should still have exactly one membership (not duplicated)
    let members = org_members::Model::find_members(db, org.id).await;
    let member_entries: Vec<_> = members.iter().filter(|m| m.user_id == member.id).collect();
    assert_eq!(
        member_entries.len(),
        1,
        "Should not create duplicate membership"
    );
    // The existing role should be unchanged (viewer, not upgraded to admin)
    assert_eq!(member_entries[0].role, "viewer");
}
