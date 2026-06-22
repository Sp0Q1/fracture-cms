//! Request tests for the blog: publish lifecycle, public visibility,
//! feed, and platform-admin gates on preview/delete.

use fracture_cms::{
    app::App,
    models::{
        org_members::{self, OrgRole},
        users::{self, OidcUserInfo},
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: format!("blog-req-{suffix}"),
            email: format!("blog-req-{suffix}@example.com"),
            name: Some(format!("BlogReq {suffix}")),
            email_verified: true,
        },
    )
    .await
    .unwrap()
}

fn jwt_cookie(
    ctx: &loco_rs::app::AppContext,
    user: &users::Model,
) -> axum_extra::extract::cookie::Cookie<'static> {
    let jwt = user
        .generate_jwt(&ctx.config.get_jwt_config().unwrap().secret, 3600)
        .unwrap();
    axum_extra::extract::cookie::Cookie::new("jwt", jwt)
}

/// Creates the blog org (slug `test-blog`, matching config/test.yaml) as a
/// platform-admin org and makes `admin` an owner of it.
async fn mk_blog_org(
    db: &sea_orm::DatabaseConnection,
    admin: &users::Model,
) -> fracture_core::models::_entities::organizations::Model {
    let org = fracture_core::models::_entities::organizations::ActiveModel {
        name: Set("Test Blog Org".into()),
        slug: Set("test-blog".into()),
        is_personal: Set(false),
        is_platform_admin: Set(true),
        settings: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    org_members::Model::add_member(db, org.id, admin.id, OrgRole::Owner)
        .await
        .unwrap();
    org
}

#[tokio::test]
#[serial]
async fn drafts_are_invisible_publicly_until_published() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = mk_user(&ctx.db, "vis-admin").await;
        mk_blog_org(&ctx.db, &admin).await;

        // Create a draft via the admin route.
        let response = request
            .post("/admin/blog")
            .add_cookie(jwt_cookie(&ctx, &admin))
            .form(&[
                ("title", "Hello World"),
                ("body", "# Hi\n\nfirst post"),
                ("excerpt", "the first post"),
            ])
            .await;
        assert_eq!(response.status_code(), 303);

        // Draft: not in the index, slug 404s, feed is empty.
        let response = request.get("/blog").await;
        assert_eq!(response.status_code(), 200);
        assert!(!response.text().contains("Hello World"));
        assert_eq!(request.get("/blog/hello-world").await.status_code(), 404);
        assert!(!request
            .get("/blog/feed.xml")
            .await
            .text()
            .contains("Hello World"));

        // Publish it.
        let post = fracture_core::models::blog_posts::Entity::find()
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        let response = request
            .post(&format!("/admin/blog/{}/publish", post.pid))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 303);

        // Now public everywhere, with cache headers on public pages.
        let response = request.get("/blog").await;
        assert!(response.text().contains("Hello World"));
        assert!(response
            .headers()
            .get("cache-control")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("public")));
        let response = request.get("/blog/hello-world").await;
        assert_eq!(response.status_code(), 200);
        assert!(response.text().contains("first post"));

        let feed = request.get("/blog/feed.xml").await;
        assert_eq!(feed.status_code(), 200);
        assert!(feed
            .headers()
            .get("content-type")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("atom")));
        assert!(feed.text().contains("Hello World"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn republish_preserves_first_published_date() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = mk_user(&ctx.db, "date-admin").await;
        mk_blog_org(&ctx.db, &admin).await;

        let response = request
            .post("/admin/blog")
            .add_cookie(jwt_cookie(&ctx, &admin))
            .form(&[("title", "Dated"), ("body", "content")])
            .await;
        assert_eq!(response.status_code(), 303);
        let post = fracture_core::models::blog_posts::Entity::find()
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();

        for action in ["publish", "unpublish", "publish"] {
            let response = request
                .post(&format!("/admin/blog/{}/{}", post.pid, action))
                .add_cookie(jwt_cookie(&ctx, &admin))
                .await;
            assert_eq!(response.status_code(), 303);
            if action == "publish" {
                // Give the second publish a distinct timestamp if it were
                // (wrongly) re-stamped.
                tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            }
        }

        let after =
            fracture_core::models::blog_posts::Model::find_by_pid(&ctx.db, &post.pid.to_string())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(after.status, "published");
        let first = after.published_at.expect("published date set");
        // The republished date must equal the first publish (within the
        // second the first publish happened), not the later republish time.
        assert!(
            chrono::Utc::now().signed_duration_since(first.with_timezone(&chrono::Utc))
                >= chrono::Duration::seconds(2),
            "published_at must not be re-stamped on republish"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn preview_and_delete_are_platform_admin_only() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = mk_user(&ctx.db, "gate-admin").await;
        let outsider = mk_user(&ctx.db, "gate-outsider").await;
        mk_blog_org(&ctx.db, &admin).await;

        let response = request
            .post("/admin/blog")
            .add_cookie(jwt_cookie(&ctx, &admin))
            .form(&[("title", "Secret Draft"), ("body", "draft body")])
            .await;
        assert_eq!(response.status_code(), 303);
        let post = fracture_core::models::blog_posts::Entity::find()
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();

        // Outsider (regular user, no platform admin org): both 403.
        let response = request
            .get(&format!("/admin/blog/{}/preview", post.pid))
            .add_cookie(jwt_cookie(&ctx, &outsider))
            .await;
        assert_eq!(response.status_code(), 403);
        let response = request
            .post(&format!("/admin/blog/{}/delete", post.pid))
            .add_cookie(jwt_cookie(&ctx, &outsider))
            .await;
        assert_eq!(response.status_code(), 403);

        // Admin preview renders the draft with a banner.
        let response = request
            .get(&format!("/admin/blog/{}/preview", post.pid))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(body.contains("draft body"));
        assert!(body.contains("Draft preview"));

        // Admin delete removes the post.
        let response = request
            .post(&format!("/admin/blog/{}/delete", post.pid))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 303);
        let gone =
            fracture_core::models::blog_posts::Model::find_by_pid(&ctx.db, &post.pid.to_string())
                .await
                .unwrap();
        assert!(gone.is_none(), "post must be deleted");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn public_pages_show_dashboard_cta_when_authenticated() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = mk_user(&ctx.db, "nav-admin").await;
        mk_blog_org(&ctx.db, &admin).await;

        // Guest: Sign in CTA, no org switcher, cacheable.
        let response = request.get("/blog").await;
        let body = response.text();
        assert!(body.contains("Sign in"));
        assert!(!body.contains(">Dashboard<"));
        assert!(
            !body.contains("id=\"org-switcher\""),
            "guests get no org switcher"
        );
        assert!(response.headers().get("cache-control").is_some());

        // Authenticated: full app nav (org switcher + account menu), NOT cacheable.
        let response = request
            .get("/blog")
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(body.contains(">Dashboard<"), "authed nav must link the app");
        assert!(!body.contains("Sign in"));
        assert!(
            body.contains("id=\"org-switcher\""),
            "authed visitors keep the org switcher on public pages"
        );
        assert!(
            response.headers().get("cache-control").is_none(),
            "session-aware variant must not be publicly cacheable"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn static_pages_render_fragments_in_public_layout() {
    request::<App, _, _>(|request, ctx| async move {
        let _ctx = ctx; // request-only test

        // The demo ships assets/views/site/pages/about.html.
        let response = request.get("/pages/about").await;
        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(body.contains("What this platform is"), "fragment content");
        assert!(body.contains("Sign in"), "wrapped in the public layout");
        assert!(response
            .headers()
            .get("cache-control")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("public")));

        // Unknown and invalid slugs are 404.
        assert_eq!(request.get("/pages/no-such-page").await.status_code(), 404);
        assert_eq!(request.get("/pages/Nope_Bad").await.status_code(), 404);
    })
    .await;
}
