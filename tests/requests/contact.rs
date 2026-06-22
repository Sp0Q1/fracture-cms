//! Request tests for the Altcha-protected contact form and admin inbox.

use fracture_cms::{
    app::App,
    models::{
        org_members::{self, OrgRole},
        users::{self, OidcUserInfo},
    },
};
use fracture_core::captcha;
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: format!("contact-{suffix}"),
            email: format!("contact-{suffix}@example.com"),
            name: Some(format!("Contact {suffix}")),
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

async fn mk_staff(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    let admin = mk_user(db, suffix).await;
    let org = fracture_core::models::_entities::organizations::ActiveModel {
        name: Set(format!("Admin Org {suffix}")),
        slug: Set(format!("admin-org-{suffix}")),
        is_personal: Set(false),
        is_staff: Set(true),
        settings: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    org_members::Model::add_member(db, org.id, admin.id, OrgRole::Owner)
        .await
        .unwrap();
    admin
}

fn solved_altcha() -> String {
    let challenge = captcha::create_challenge();
    captcha::solve_challenge(&challenge).expect("challenge is solvable")
}

#[tokio::test]
#[serial]
async fn contact_form_renders_with_widget() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/contact").await;
        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(body.contains("altcha-widget"));
        assert!(body.contains("/captcha/challenge"));

        // The challenge endpoint serves valid, non-cacheable JSON.
        let response = request.get("/captcha/challenge").await;
        assert_eq!(response.status_code(), 200);
        assert!(response
            .headers()
            .get("cache-control")
            .is_some_and(|v| v.to_str().unwrap_or("") == "no-store"));
        let body = response.text();
        assert!(body.contains("\"algorithm\":\"SHA-256\""));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn submission_without_valid_captcha_is_rejected() {
    request::<App, _, _>(|request, ctx| async move {
        for altcha in ["", "bm90IGEgcmVhbCBwYXlsb2Fk"] {
            let response = request
                .post("/contact")
                .form(&[
                    ("name", "Mallory"),
                    ("email", "mallory@example.com"),
                    ("message", "spam"),
                    ("altcha", altcha),
                ])
                .await;
            assert_eq!(response.status_code(), 400);
        }
        let stored = fracture_core::models::contact_messages::Model::find_recent(&ctx.db, 10)
            .await
            .unwrap();
        assert!(stored.is_empty(), "nothing may be stored without captcha");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn solved_captcha_stores_message_and_cannot_be_replayed() {
    request::<App, _, _>(|request, ctx| async move {
        let altcha = solved_altcha();
        let response = request
            .post("/contact")
            .form(&[
                ("name", "Alice"),
                ("email", "alice@example.com"),
                ("message", "Hello, I would like a demo."),
                ("altcha", &altcha),
            ])
            .await;
        assert_eq!(response.status_code(), 303);

        let stored = fracture_core::models::contact_messages::Model::find_recent(&ctx.db, 10)
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].email, "alice@example.com");

        // The same solved payload must not work twice.
        let response = request
            .post("/contact")
            .form(&[
                ("name", "Alice"),
                ("email", "alice@example.com"),
                ("message", "replay"),
                ("altcha", &altcha),
            ])
            .await;
        assert_eq!(response.status_code(), 400, "replays must be rejected");

        // Field validation still applies after a fresh captcha.
        let fresh = solved_altcha();
        let response = request
            .post("/contact")
            .form(&[
                ("name", "Alice"),
                ("email", "not-an-email"),
                ("message", "x"),
                ("altcha", &fresh),
            ])
            .await;
        assert_eq!(response.status_code(), 400);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn admin_inbox_is_staff_only() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = mk_staff(&ctx.db, "inbox-admin").await;
        let outsider = mk_user(&ctx.db, "inbox-outsider").await;

        let altcha = solved_altcha();
        let response = request
            .post("/contact")
            .form(&[
                ("name", "Bob"),
                ("email", "bob@example.com"),
                ("message", "secret inquiry"),
                ("altcha", &altcha),
            ])
            .await;
        assert_eq!(response.status_code(), 303);

        // Outsider cannot read or delete.
        let response = request
            .get("/admin/contact")
            .add_cookie(jwt_cookie(&ctx, &outsider))
            .await;
        assert_eq!(response.status_code(), 403);

        // Admin sees the message.
        let response = request
            .get("/admin/contact")
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 200);
        assert!(response.text().contains("secret inquiry"));

        // Admin deletes it.
        let msg = &fracture_core::models::contact_messages::Model::find_recent(&ctx.db, 10)
            .await
            .unwrap()[0];
        let response = request
            .post(&format!("/admin/contact/{}/delete", msg.pid))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;
        assert_eq!(response.status_code(), 303);
        let remaining = fracture_core::models::contact_messages::Model::find_recent(&ctx.db, 10)
            .await
            .unwrap();
        assert!(remaining.is_empty());
    })
    .await;
}
