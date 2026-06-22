//! Tests for the blog post model: markdown rendering safety.

use fracture_cms::app::App;
use fracture_cms::models::users::{self, OidcUserInfo};
use fracture_core::models::_entities::blog_posts;
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn markdown_rendering_strips_raw_html() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let author = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: "blog-md".into(),
            email: "blog-md@example.com".into(),
            name: Some("Blog MD".into()),
            email_verified: true,
        },
    )
    .await
    .unwrap();
    let org = crate::support::owned_org(db, "blog-md", author.id).await;

    let post = blog_posts::ActiveModel {
        org_id: Set(org.id),
        author_id: Set(author.id),
        title: Set("xss test".into()),
        slug: Set("xss-test".into()),
        body: Set("# Hello\n\n<script>alert(1)</script>\n\n**bold**".into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    // comrak renders with unsafe=false: markdown becomes HTML, but raw HTML
    // in the source is neutralized.
    assert!(post.body_html.contains("<h1>"), "markdown must render");
    assert!(post.body_html.contains("<strong>bold</strong>"));
    assert!(
        !post.body_html.contains("<script>"),
        "raw HTML must not pass through: {}",
        post.body_html
    );
}
