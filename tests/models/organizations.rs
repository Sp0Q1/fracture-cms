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

    // Joining the deployment's default org adds them at the configured role,
    // idempotently (a second call doesn't change or duplicate the membership).
    let org = organizations::Model::ensure_default_membership(
        db,
        "acme",
        "Acme Inc.",
        OrgRole::Member,
        user.id,
    )
    .await
    .unwrap()
    .expect("brand-new user is placed in the default org");
    // Second call: the user now has an org, so it is a no-op (returns None).
    let repeat = organizations::Model::ensure_default_membership(
        db,
        "acme",
        "Acme Inc.",
        OrgRole::Member,
        user.id,
    )
    .await
    .unwrap();
    assert!(
        repeat.is_none(),
        "user already has an org, so a repeat call is a no-op"
    );
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    assert_eq!(orgs.len(), 1, "joined exactly the default org once");
    assert!(!orgs[0].is_personal);
    let membership = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .unwrap()
        .expect("default-org membership");
    assert_eq!(
        membership.role, "member",
        "default-org join uses given role"
    );
}

#[tokio::test]
#[serial]
async fn test_default_org_skipped_when_user_already_has_an_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // A user who already belongs to an org (e.g. joined via an auto-accepted
    // invite, or staff via their staff org) must NOT also be dropped into the
    // shared default org.
    let user = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: "already-homed".to_string(),
            email: "already-homed@example.com".to_string(),
            name: Some("Already Homed".to_string()),
            email_verified: true,
        },
    )
    .await
    .unwrap();
    let home = crate::support::owned_org(db, "already-homed", user.id).await;

    let result = organizations::Model::ensure_default_membership(
        db,
        "acme",
        "Acme Inc.",
        OrgRole::Member,
        user.id,
    )
    .await
    .unwrap();
    assert!(
        result.is_none(),
        "user with an existing org is not added to the default org"
    );

    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    assert_eq!(orgs.len(), 1, "still in exactly their original org");
    assert_eq!(orgs[0].id, home.id, "unchanged home org");

    // The default org was never created, since no homeless user needed it.
    assert!(
        organizations::Model::find_by_slug(db, "acme")
            .await
            .unwrap()
            .is_none(),
        "default org is created lazily, only for a user who needs a home"
    );
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

#[tokio::test]
#[serial]
async fn ensure_staff_membership_creates_staff_org_and_grants_staff() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // A user who already belongs to a (non-staff) client org.
    let user = create_test_user(db, "staffprov").await;
    assert!(
        !organizations::Model::is_user_staff(db, user.id).await,
        "not staff before provisioning"
    );

    // Provisioning creates the staff org (is_staff=true) and adds the user as
    // Owner — so they are now recognized as staff.
    let org =
        organizations::Model::ensure_staff_membership(db, "platform-admin", "Platform", user.id)
            .await
            .unwrap();
    assert!(org.is_staff, "staff org is flagged is_staff");
    assert!(
        organizations::Model::is_user_staff(db, user.id).await,
        "user is recognized as staff after provisioning"
    );
    let m = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .unwrap()
        .expect("staff-org membership");
    assert_eq!(m.role, "owner");

    // Idempotent: a repeat call reuses the org and adds no duplicate membership.
    let org2 =
        organizations::Model::ensure_staff_membership(db, "platform-admin", "Platform", user.id)
            .await
            .unwrap();
    assert_eq!(org2.id, org.id);
    assert_eq!(
        org_members::Model::find_members(db, org.id)
            .await
            .unwrap()
            .len(),
        1,
        "no duplicate staff membership"
    );
}
