use fracture_cms::{
    app::App,
    models::{
        staff_org_access,
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, IntoActiveModel};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: format!("soa-{suffix}"),
            email: format!("soa-{suffix}@example.com"),
            name: Some(format!("SOA {suffix}")),
            email_verified: true,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
#[serial]
async fn record_is_idempotent_and_tracks_first_and_last_access() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = mk_user(db, "owner").await;
    let org = crate::support::owned_org(db, "soa", owner.id).await;
    let staff = mk_user(db, "staff").await;

    // First access: one record, first == last, paired with the right user.
    staff_org_access::Model::record(db, org.id, staff.id)
        .await
        .unwrap();
    let rows = staff_org_access::Model::find_for_org_with_users(db, org.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "one access record after first access");
    let (rec, u) = &rows[0];
    assert_eq!(u.id, staff.id, "record is paired with the staff user");
    assert_eq!(
        rec.first_accessed_at, rec.last_active_at,
        "first access stamps both timestamps equally"
    );
    let first = rec.first_accessed_at;

    // Backdate last_active_at, then record again: still one row, first preserved,
    // last_active_at advances.
    let past = first - chrono::Duration::hours(1);
    let mut active = rec.clone().into_active_model();
    active.last_active_at = Set(past);
    active.update(db).await.unwrap();

    staff_org_access::Model::record(db, org.id, staff.id)
        .await
        .unwrap();
    let rows = staff_org_access::Model::find_for_org_with_users(db, org.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "repeat access does not create a second row");
    let (rec2, _) = &rows[0];
    assert_eq!(
        rec2.first_accessed_at, first,
        "first_accessed_at is preserved across accesses"
    );
    assert!(
        rec2.last_active_at > past,
        "last_active_at advances on repeat access"
    );
}

#[tokio::test]
#[serial]
async fn records_are_scoped_per_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let owner = mk_user(db, "scope-owner").await;
    let org_a = crate::support::owned_org(db, "scope-a", owner.id).await;
    let org_b = crate::support::owned_org(db, "scope-b", owner.id).await;
    let staff = mk_user(db, "scope-staff").await;

    staff_org_access::Model::record(db, org_a.id, staff.id)
        .await
        .unwrap();

    assert_eq!(
        staff_org_access::Model::find_for_org_with_users(db, org_a.id)
            .await
            .unwrap()
            .len(),
        1,
        "access to org A is recorded for org A"
    );
    assert!(
        staff_org_access::Model::find_for_org_with_users(db, org_b.id)
            .await
            .unwrap()
            .is_empty(),
        "org B shows no access — the staffer never touched it"
    );
}
