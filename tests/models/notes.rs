use fracture_cms::{
    app::App,
    models::{
        _entities::{
            notes::{ActiveModel as NoteActiveModel, Model as NoteModel},
            projects::ActiveModel as ProjectActiveModel,
        },
        organizations,
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    let user = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-note-{suffix}"),
            email: format!("note-{suffix}@example.com"),
            name: Some(format!("Note User {suffix}")),
            email_verified: true,
        },
    )
    .await
    .expect("create test user");
    crate::support::owned_org(db, suffix, user.id).await;
    user
}

async fn create_project(
    db: &sea_orm::DatabaseConnection,
    title: &str,
    org_id: i32,
) -> fracture_cms::models::_entities::projects::Model {
    ProjectActiveModel {
        title: Set(title.to_string()),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("Failed to create project")
}

async fn create_note(
    db: &sea_orm::DatabaseConnection,
    title: &str,
    project_id: i32,
    org_id: i32,
) -> NoteModel {
    NoteActiveModel {
        title: Set(title.to_string()),
        project_id: Set(project_id),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("Failed to create note")
}

#[tokio::test]
#[serial]
async fn test_find_by_project_and_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "notelist").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];
    let project = create_project(db, "Note Project", org.id).await;

    create_note(db, "Note 1", project.id, org.id).await;
    create_note(db, "Note 2", project.id, org.id).await;

    let notes = NoteModel::find_by_project_and_org(db, project.id, org.id)
        .await
        .unwrap();
    assert_eq!(notes.len(), 2);
    // Ordered by id DESC
    assert_eq!(notes[0].title, "Note 2");
    assert_eq!(notes[1].title, "Note 1");
}

#[tokio::test]
#[serial]
async fn test_find_by_pid_and_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "notepid").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];
    let project = create_project(db, "Note PID Project", org.id).await;
    let note = create_note(db, "Find Me", project.id, org.id).await;

    let found = NoteModel::find_by_pid_and_org(db, &note.pid.to_string(), org.id)
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "Find Me");

    // Wrong org
    let not_found = NoteModel::find_by_pid_and_org(db, &note.pid.to_string(), org.id + 999)
        .await
        .unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
#[serial]
async fn test_note_sets_pid_on_insert() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "notepidset").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];
    let project = create_project(db, "PID Note Project", org.id).await;
    let note = create_note(db, "PID Note", project.id, org.id).await;

    assert!(!note.pid.is_nil());
}

#[tokio::test]
#[serial]
async fn test_notes_cross_org_isolation() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "note-iso-alice").await;
    let bob = create_test_user(db, "note-iso-bob").await;

    let alice_orgs = organizations::Model::find_orgs_for_user(db, alice.id)
        .await
        .unwrap();
    let alice_org = &alice_orgs[0];
    let bob_orgs = organizations::Model::find_orgs_for_user(db, bob.id)
        .await
        .unwrap();
    let bob_org = &bob_orgs[0];

    let alice_project = create_project(db, "Alice's NP", alice_org.id).await;
    let bob_project = create_project(db, "Bob's NP", bob_org.id).await;

    let alice_note = create_note(db, "Alice's Note", alice_project.id, alice_org.id).await;
    let _bob_note = create_note(db, "Bob's Note", bob_project.id, bob_org.id).await;

    // Alice's notes not visible from Bob's org
    assert!(
        NoteModel::find_by_pid_and_org(db, &alice_note.pid.to_string(), bob_org.id)
            .await
            .unwrap()
            .is_none()
    );

    // Bob's project notes don't show up in Alice's org
    let alice_notes = NoteModel::find_by_project_and_org(db, bob_project.id, alice_org.id)
        .await
        .unwrap();
    assert!(alice_notes.is_empty());
}

#[tokio::test]
#[serial]
async fn test_notes_cross_project_isolation_within_same_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "note-crossproj").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    let project_a = create_project(db, "Project A", org.id).await;
    let project_b = create_project(db, "Project B", org.id).await;

    create_note(db, "Note in A", project_a.id, org.id).await;
    create_note(db, "Note in B", project_b.id, org.id).await;

    // Each project should only see its own notes
    let notes_a = NoteModel::find_by_project_and_org(db, project_a.id, org.id)
        .await
        .unwrap();
    assert_eq!(notes_a.len(), 1);
    assert_eq!(notes_a[0].title, "Note in A");

    let notes_b = NoteModel::find_by_project_and_org(db, project_b.id, org.id)
        .await
        .unwrap();
    assert_eq!(notes_b.len(), 1);
    assert_eq!(notes_b[0].title, "Note in B");
}

#[tokio::test]
#[serial]
async fn test_note_requires_valid_project_id() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "note-badproj").await;
    let orgs = organizations::Model::find_orgs_for_user(db, user.id)
        .await
        .unwrap();
    let org = &orgs[0];

    // Create a note with a nonexistent project_id — should fail due to FK constraint
    let result = NoteActiveModel {
        title: Set("Orphan Note".to_string()),
        project_id: Set(99999),
        org_id: Set(org.id),
        ..Default::default()
    }
    .insert(db)
    .await;

    assert!(
        result.is_err(),
        "Creating a note with a nonexistent project should fail"
    );
}
