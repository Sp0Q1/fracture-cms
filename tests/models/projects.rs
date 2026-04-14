use fracture_cms::{
    app::App,
    models::{
        _entities::projects::{ActiveModel, Model},
        organizations,
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-proj-{suffix}"),
            email: format!("proj-{suffix}@example.com"),
            name: Some(format!("Proj User {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

async fn create_project(db: &sea_orm::DatabaseConnection, title: &str, org_id: i32) -> Model {
    ActiveModel {
        title: Set(title.to_string()),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("Failed to create project")
}

#[tokio::test]
#[serial]
async fn test_find_by_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "orglist").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await.unwrap();
    let org = &orgs[0];

    create_project(db, "Project A", org.id).await;
    create_project(db, "Project B", org.id).await;

    let projects = Model::find_by_org(db, org.id).await.unwrap();
    assert_eq!(projects.len(), 2);
    // Ordered by id DESC
    assert_eq!(projects[0].title, "Project B");
    assert_eq!(projects[1].title, "Project A");
}

#[tokio::test]
#[serial]
async fn test_find_by_pid_and_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "pidorg").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await.unwrap();
    let org = &orgs[0];

    let project = create_project(db, "Scoped Project", org.id).await;

    let found = Model::find_by_pid_and_org(db, &project.pid.to_string(), org.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "Scoped Project");

    // Wrong org should return None
    let not_found = Model::find_by_pid_and_org(db, &project.pid.to_string(), org.id + 999).await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
#[serial]
async fn test_cross_org_isolation() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "iso-alice").await;
    let bob = create_test_user(db, "iso-bob").await;

    let alice_orgs = organizations::Model::find_orgs_for_user(db, alice.id).await.unwrap();
    let alice_org = &alice_orgs[0];
    let bob_orgs = organizations::Model::find_orgs_for_user(db, bob.id).await.unwrap();
    let bob_org = &bob_orgs[0];

    let alice_project = create_project(db, "Alice's Project", alice_org.id).await;
    let bob_project = create_project(db, "Bob's Project", bob_org.id).await;

    // Alice's org lists only Alice's project
    let alice_list = Model::find_by_org(db, alice_org.id).await.unwrap();
    assert_eq!(alice_list.len(), 1);
    assert_eq!(alice_list[0].id, alice_project.id);

    // Bob's org lists only Bob's project
    let bob_list = Model::find_by_org(db, bob_org.id).await.unwrap();
    assert_eq!(bob_list.len(), 1);
    assert_eq!(bob_list[0].id, bob_project.id);

    // Cross-org lookup fails
    assert!(
        Model::find_by_pid_and_org(db, &alice_project.pid.to_string(), bob_org.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        Model::find_by_pid_and_org(db, &bob_project.pid.to_string(), alice_org.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[serial]
async fn test_project_sets_pid_on_insert() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "setpid").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await.unwrap();
    let org = &orgs[0];

    let project = create_project(db, "PID Test", org.id).await;
    assert!(!project.pid.is_nil());
}

#[tokio::test]
#[serial]
async fn test_project_requires_valid_org_id() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    // Creating a project with a nonexistent org_id should fail due to FK constraint
    let result = ActiveModel {
        title: Set("Orphan Project".to_string()),
        org_id: Set(99999),
        ..Default::default()
    }
    .insert(db)
    .await;

    assert!(
        result.is_err(),
        "Creating a project with a nonexistent org should fail"
    );
}

#[tokio::test]
#[serial]
async fn test_find_by_pid_returns_none_for_invalid_uuid() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "badpid").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await.unwrap();
    let org = &orgs[0];

    // Invalid UUID string
    let result = Model::find_by_pid_and_org(db, "not-a-valid-uuid", org.id).await.unwrap();
    assert!(result.is_none(), "Invalid UUID should return None");

    // Valid UUID but nonexistent
    let result =
        Model::find_by_pid_and_org(db, "00000000-0000-0000-0000-000000000000", org.id).await.unwrap();
    assert!(result.is_none(), "Nonexistent UUID should return None");
}
