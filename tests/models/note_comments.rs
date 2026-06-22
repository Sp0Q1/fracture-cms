use fracture_cms::{
    app::App,
    models::{
        _entities::{
            note_comments::ActiveModel as CommentActiveModel,
            notes::ActiveModel as NoteActiveModel, projects::ActiveModel as ProjectActiveModel,
        },
        organizations,
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-comment-{suffix}"),
            email: format!("comment-{suffix}@example.com"),
            name: Some(format!("Comment User {suffix}")),
            email_verified: true,
        },
    )
    .await
    .expect("create test user")
}

/// Comments: the author (or staff) can edit/delete; other org users can't;
/// and Members can `COMMENT` on a note while Viewers cannot.
#[tokio::test]
#[serial]
async fn comment_edit_delete_is_author_or_staff_only() {
    use fracture_cms::models::org_members::OrgRole;
    use fracture_cms::permissions::{capabilities, COMMENT, DELETE, EDIT, VIEW};

    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let author = mk_user(db, "author").await;
    let other = mk_user(db, "other").await;
    // Create an org directly (the capability checks below take role explicitly,
    // so no membership rows are needed).
    let org = organizations::ActiveModel {
        name: Set("Auth Org".to_string()),
        slug: Set("auth-comments".to_string()),
        is_personal: Set(false),
        is_staff: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    let project = ProjectActiveModel {
        title: Set("P".to_string()),
        org_id: Set(org.id),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    let note = NoteActiveModel {
        title: Set("N".to_string()),
        project_id: Set(project.id),
        org_id: Set(org.id),
        owner_tier: Set("staff".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    let comment = CommentActiveModel {
        note_id: Set(note.id),
        org_id: Set(org.id),
        author_id: Set(author.id),
        body: Set("hello".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    // The author can edit and delete their own comment.
    let caps = capabilities(db, author.id, false, OrgRole::Member, &comment)
        .await
        .unwrap();
    assert!(
        caps.allows(EDIT) && caps.allows(DELETE),
        "author controls own"
    );

    // Another org user (even an Owner) can read but not edit/delete it.
    let caps = capabilities(db, other.id, false, OrgRole::Owner, &comment)
        .await
        .unwrap();
    assert!(caps.allows(VIEW));
    assert!(
        !caps.allows(EDIT) && !caps.allows(DELETE),
        "non-author org users can't edit/delete others' comments"
    );

    // Staff (platform admin) can edit/delete any comment.
    let caps = capabilities(db, other.id, true, OrgRole::Viewer, &comment)
        .await
        .unwrap();
    assert!(caps.allows(EDIT) && caps.allows(DELETE));

    // COMMENT on the note: Members may, Viewers may not.
    let note_caps = capabilities(db, other.id, false, OrgRole::Member, &note)
        .await
        .unwrap();
    assert!(note_caps.allows(COMMENT), "members can comment");
    let viewer_caps = capabilities(db, other.id, false, OrgRole::Viewer, &note)
        .await
        .unwrap();
    assert!(!viewer_caps.allows(COMMENT), "viewers cannot comment");
}
