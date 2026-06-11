//! Tests for the generic `ResourceAssignment` model.
//!
//! These tests are the contract that downstream crates rely on: if any of
//! them fail, IDOR prevention in downstream consumers breaks.

use chrono::{Duration, Utc};
use fracture_cms::{
    app::App,
    models::{
        organizations,
        resource_assignments::{self, AssignParams, AssignmentError},
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use serial_test::serial;

const RESOURCE_TYPE: &str = "test_resource";
const ROLE_KEY: &str = "test_role";

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-ra-{suffix}"),
            email: format!("ra-{suffix}@example.com"),
            name: Some(format!("RA User {suffix}")),
            email_verified: true,
        },
    )
    .await
    .expect("Failed to create test user")
}

async fn personal_org(db: &sea_orm::DatabaseConnection, user_id: i32) -> organizations::Model {
    organizations::Model::find_orgs_for_user(db, user_id)
        .await
        .expect("Failed to load orgs for user")
        .into_iter()
        .next()
        .expect("user should have a personal org from creation")
}

#[tokio::test]
#[serial]
async fn test_assign_then_has_assignment() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "assign-1").await;
    let org = personal_org(db, user.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 42,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .expect("assign should succeed");

    let granted =
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 42, ROLE_KEY)
            .await
            .unwrap();
    assert!(granted);
}

#[tokio::test]
#[serial]
async fn test_has_assignment_false_for_other_resource_id() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "isolate-rid").await;
    let org = personal_org(db, user.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 1,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    // Assignment on id=1 must NOT grant access to id=2.
    let other =
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 2, ROLE_KEY)
            .await
            .unwrap();
    assert!(!other, "assignment must not leak across resource ids");
}

#[tokio::test]
#[serial]
async fn test_has_assignment_false_for_other_resource_type() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "isolate-type").await;
    let org = personal_org(db, user.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: "engagement",
            resource_id: 7,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let cross_type =
        resource_assignments::Model::has_assignment(db, user.id, "report", 7, ROLE_KEY)
            .await
            .unwrap();
    assert!(
        !cross_type,
        "assignment must not leak across resource types"
    );
}

#[tokio::test]
#[serial]
async fn test_has_assignment_false_for_other_user() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "ucross-alice").await;
    let bob = create_test_user(db, "ucross-bob").await;
    let alice_org = personal_org(db, alice.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: alice.id,
            org_id: alice_org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 100,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let bob_has =
        resource_assignments::Model::has_assignment(db, bob.id, RESOURCE_TYPE, 100, ROLE_KEY)
            .await
            .unwrap();
    assert!(!bob_has, "assignment must not leak to other users");
}

#[tokio::test]
#[serial]
async fn test_revoke_clears_active_assignment() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "revoke-1").await;
    let org = personal_org(db, user.id).await;

    let row = resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 11,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    assert!(
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 11, ROLE_KEY)
            .await
            .unwrap()
    );

    let revoked = row.revoke(db).await.unwrap();
    assert!(revoked.revoked_at.is_some());

    assert!(
        !resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 11, ROLE_KEY)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[serial]
async fn test_revoke_is_idempotent() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "revoke-2x").await;
    let org = personal_org(db, user.id).await;

    let row = resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 12,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let once = row.revoke(db).await.unwrap();
    let twice = once.clone().revoke(db).await.unwrap();
    assert_eq!(once.revoked_at, twice.revoked_at);
}

#[tokio::test]
#[serial]
async fn test_expired_assignment_not_active() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "expired").await;
    let org = personal_org(db, user.id).await;

    let past = (Utc::now() - Duration::days(1)).into();
    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 13,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: Some(past),
        },
    )
    .await
    .unwrap();

    let active =
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 13, ROLE_KEY)
            .await
            .unwrap();
    assert!(!active, "expired assignment must not be active");
}

#[tokio::test]
#[serial]
async fn test_future_expiry_still_active() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "future-exp").await;
    let org = personal_org(db, user.id).await;

    let future = (Utc::now() + Duration::days(30)).into();
    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 14,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: Some(future),
        },
    )
    .await
    .unwrap();

    assert!(
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 14, ROLE_KEY)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[serial]
async fn test_duplicate_active_assignment_rejected() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "dupe").await;
    let org = personal_org(db, user.id).await;

    let params = AssignParams {
        user_id: user.id,
        org_id: org.id,
        resource_type: RESOURCE_TYPE,
        resource_id: 15,
        role_key: ROLE_KEY,
        granted_by: None,
        expires_at: None,
    };

    resource_assignments::Model::assign(db, params.clone())
        .await
        .unwrap();

    let dup = resource_assignments::Model::assign(db, params).await;
    assert!(matches!(dup, Err(AssignmentError::AlreadyAssigned)));
}

#[tokio::test]
#[serial]
async fn test_can_reassign_after_revoke() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "reassign").await;
    let org = personal_org(db, user.id).await;

    let params = AssignParams {
        user_id: user.id,
        org_id: org.id,
        resource_type: RESOURCE_TYPE,
        resource_id: 16,
        role_key: ROLE_KEY,
        granted_by: None,
        expires_at: None,
    };

    let row = resource_assignments::Model::assign(db, params.clone())
        .await
        .unwrap();
    row.revoke(db).await.unwrap();

    // After revoke, a new active assignment should succeed.
    resource_assignments::Model::assign(db, params)
        .await
        .expect("re-assigning after revoke should succeed");
}

#[tokio::test]
#[serial]
async fn test_list_for_resource_returns_active_only() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "list-r-a").await;
    let bob = create_test_user(db, "list-r-b").await;
    let alice_org = personal_org(db, alice.id).await;

    let alice_row = resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: alice.id,
            org_id: alice_org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 50,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: bob.id,
            org_id: alice_org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 50,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let active = resource_assignments::Model::list_for_resource(db, RESOURCE_TYPE, 50)
        .await
        .unwrap();
    assert_eq!(active.len(), 2);

    alice_row.revoke(db).await.unwrap();

    let after = resource_assignments::Model::list_for_resource(db, RESOURCE_TYPE, 50)
        .await
        .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].user_id, bob.id);

    // History should still see the revoked row.
    let history = resource_assignments::Model::list_history_for_resource(db, RESOURCE_TYPE, 50)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
}

#[tokio::test]
#[serial]
async fn test_list_for_user_filters_by_type() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "list-u").await;
    let org = personal_org(db, user.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: "engagement",
            resource_id: 1,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: "engagement",
            resource_id: 2,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: "report",
            resource_id: 9,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let engagements = resource_assignments::Model::list_for_user(db, user.id, "engagement")
        .await
        .unwrap();
    assert_eq!(engagements.len(), 2);
    let reports = resource_assignments::Model::list_for_user(db, user.id, "report")
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);

    let ids = resource_assignments::Model::assigned_resource_ids(db, user.id, "engagement")
        .await
        .unwrap();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![1, 2]);
}

#[tokio::test]
#[serial]
async fn test_find_by_pid_round_trip() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "pid-rt").await;
    let org = personal_org(db, user.id).await;

    let row = resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 99,
            role_key: ROLE_KEY,
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let found = resource_assignments::Model::find_by_pid(db, &row.pid.to_string())
        .await
        .unwrap()
        .expect("should round-trip");
    assert_eq!(found.id, row.id);

    // Invalid pid
    let bad = resource_assignments::Model::find_by_pid(db, "not-a-uuid")
        .await
        .unwrap();
    assert!(bad.is_none());
}

#[tokio::test]
#[serial]
async fn test_has_any_assignment() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "any").await;
    let org = personal_org(db, user.id).await;

    resource_assignments::Model::assign(
        db,
        AssignParams {
            user_id: user.id,
            org_id: org.id,
            resource_type: RESOURCE_TYPE,
            resource_id: 77,
            role_key: "role_a",
            granted_by: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();

    // has_any_assignment is true regardless of which role we ask about
    let any = resource_assignments::Model::has_any_assignment(db, user.id, RESOURCE_TYPE, 77)
        .await
        .unwrap();
    assert!(any);

    // has_assignment for the wrong role_key is false
    let specific =
        resource_assignments::Model::has_assignment(db, user.id, RESOURCE_TYPE, 77, "role_b")
            .await
            .unwrap();
    assert!(!specific);
}
