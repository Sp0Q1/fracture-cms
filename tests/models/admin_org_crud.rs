use std::collections::HashMap;

use fracture_cms::app::App;
use fracture_cms::models::_entities::organizations;
use fracture_cms::models::org_members::{self, OrgRole};
use fracture_cms::models::users::{self, OidcUserInfo};
use fracture_core::entity_registry::{AdminEntity, OrgsEntity};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, sub: &str, email: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: sub.to_string(),
            email: email.to_string(),
            name: Some(sub.to_string()),
            email_verified: true,
        },
    )
    .await
    .expect("create test user")
}

async fn mk_org(
    db: &sea_orm::DatabaseConnection,
    name: &str,
    slug: &str,
    is_staff: bool,
) -> organizations::Model {
    organizations::ActiveModel {
        name: Set(name.to_string()),
        slug: Set(slug.to_string()),
        is_personal: Set(false),
        is_staff: Set(is_staff),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("create test org")
}

/// The generic admin org form exposes edit + detail but hides "Add" (org
/// creation must assign an owner via the dedicated flow).
#[tokio::test]
#[serial]
async fn orgs_entity_form_flags() {
    let _boot = boot_test::<App>().await.unwrap();
    assert!(OrgsEntity.editable(), "orgs are editable");
    assert!(
        !OrgsEntity.creatable(),
        "orgs are not creatable via generic form"
    );
    assert!(
        OrgsEntity.form_fields().iter().any(|f| f.name == "name"),
        "name is an editable field"
    );
}

/// load() returns the displayable fields and update() renames the org.
#[tokio::test]
#[serial]
async fn orgs_entity_load_and_update() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "Crud Co", "crud-co", false).await;
    let pid = org.pid.to_string();

    let record = OrgsEntity.load(db, &pid).await.unwrap().expect("loaded");
    assert_eq!(record["name"], "Crud Co");
    assert_eq!(record["slug"], "crud-co");

    let mut form = HashMap::new();
    form.insert("name".to_string(), "Renamed Co".to_string());
    OrgsEntity.update(db, &pid, &form).await.unwrap();

    let reloaded = OrgsEntity.load(db, &pid).await.unwrap().expect("loaded");
    assert_eq!(reloaded["name"], "Renamed Co");
    // The slug is derived and must not change on rename.
    assert_eq!(reloaded["slug"], "crud-co");
}

/// update() rejects an empty name with a user-visible message.
#[tokio::test]
#[serial]
async fn orgs_entity_update_requires_name() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "Needs Name", "needs-name", false).await;
    let mut form = HashMap::new();
    form.insert("name".to_string(), "   ".to_string());
    let err = OrgsEntity
        .update(db, &org.pid.to_string(), &form)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Name is required"));
}

/// delete() enforces its invariants: refuse the staff org, refuse when a member
/// would be left orgless, succeed otherwise.
#[tokio::test]
#[serial]
async fn orgs_entity_delete_guards() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // Staff org: never deletable.
    let staff = mk_org(db, "Staff Org", "staff-guard", true).await;
    let err = OrgsEntity
        .delete(db, &staff.pid.to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("staff organization"));

    // Org whose sole member has no other org: refused.
    let only = mk_org(db, "Only Org", "only-guard", false).await;
    let user = mk_user(db, "only-member", "only-member@example.com").await;
    org_members::Model::add_member(db, only.id, user.id, OrgRole::Owner)
        .await
        .unwrap();
    let err = OrgsEntity
        .delete(db, &only.pid.to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no other organization"));

    // Memberless, non-staff org: deletes cleanly.
    let throwaway = mk_org(db, "Throwaway", "throwaway-guard", false).await;
    OrgsEntity
        .delete(db, &throwaway.pid.to_string())
        .await
        .unwrap();
    assert!(
        OrgsEntity
            .load(db, &throwaway.pid.to_string())
            .await
            .unwrap()
            .is_none(),
        "deleted org should be gone"
    );
}
