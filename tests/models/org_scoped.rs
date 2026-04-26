//! Tests for the `OrgScoped` trait — IDOR prevention via cross-org isolation.

use fracture_cms::{
    app::App,
    models::{
        _entities::{blog_posts, uploads},
        organizations,
        users::{self, OidcUserInfo},
        OrgScopedQuery,
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serial_test::serial;

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-os-{suffix}"),
            email: format!("os-{suffix}@example.com"),
            name: Some(format!("OS User {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

#[tokio::test]
#[serial]
async fn test_org_scoped_blog_posts_cross_org_isolation() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "blog-alice").await;
    let bob = create_test_user(db, "blog-bob").await;

    let alice_org = organizations::Model::find_orgs_for_user(db, alice.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let bob_org = organizations::Model::find_orgs_for_user(db, bob.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    blog_posts::ActiveModel {
        org_id: Set(alice_org.id),
        author_id: Set(alice.id),
        title: Set("Alice draft".to_string()),
        slug: Set("alice-draft".to_string()),
        body: Set(String::new()),
        body_html: Set(String::new()),
        status: Set("draft".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    blog_posts::ActiveModel {
        org_id: Set(bob_org.id),
        author_id: Set(bob.id),
        title: Set("Bob draft".to_string()),
        slug: Set("bob-draft".to_string()),
        body: Set(String::new()),
        body_html: Set(String::new()),
        status: Set("draft".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    // OrgScoped::find_in_org returns only the requested org's rows.
    let alice_view = blog_posts::Entity::find_in_org(alice_org.id)
        .all(db)
        .await
        .unwrap();
    assert_eq!(alice_view.len(), 1);
    assert_eq!(alice_view[0].title, "Alice draft");

    let bob_view = blog_posts::Entity::find_in_org(bob_org.id)
        .all(db)
        .await
        .unwrap();
    assert_eq!(bob_view.len(), 1);
    assert_eq!(bob_view[0].title, "Bob draft");
}

#[tokio::test]
#[serial]
async fn test_org_scoped_uploads_returns_only_owning_org() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "up-alice").await;
    let bob = create_test_user(db, "up-bob").await;

    let alice_org = organizations::Model::find_orgs_for_user(db, alice.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let bob_org = organizations::Model::find_orgs_for_user(db, bob.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    uploads::ActiveModel {
        org_id: Set(alice_org.id),
        uploaded_by: Set(alice.id),
        original_name: Set("alice.png".to_string()),
        storage_path: Set("/tmp/alice.png".to_string()),
        content_type: Set("image/png".to_string()),
        size_bytes: Set(123),
        visibility: Set("org".to_string()),
        checksum_sha256: Set("a".repeat(64)),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    uploads::ActiveModel {
        org_id: Set(bob_org.id),
        uploaded_by: Set(bob.id),
        original_name: Set("bob.png".to_string()),
        storage_path: Set("/tmp/bob.png".to_string()),
        content_type: Set("image/png".to_string()),
        size_bytes: Set(456),
        visibility: Set("org".to_string()),
        checksum_sha256: Set("b".repeat(64)),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    let alice_uploads = uploads::Entity::find_in_org(alice_org.id)
        .all(db)
        .await
        .unwrap();
    assert_eq!(alice_uploads.len(), 1);
    assert_eq!(alice_uploads[0].original_name, "alice.png");

    let bob_uploads = uploads::Entity::find_in_org(bob_org.id)
        .all(db)
        .await
        .unwrap();
    assert_eq!(bob_uploads.len(), 1);
    assert_eq!(bob_uploads[0].original_name, "bob.png");
}
