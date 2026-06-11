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

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: format!("req-{suffix}"),
            email: format!("req-{suffix}@example.com"),
            name: Some(format!("Req {suffix}")),
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

#[tokio::test]
#[serial]
async fn orgs_list_returns_user_orgs_when_authenticated() {
    request::<App, _, _>(|request, ctx| async move {
        // Create a user (this also creates a personal org)
        let user = users::Model::find_or_create_from_oidc(
            &ctx.db,
            &OidcUserInfo {
                provider: "test".into(),
                subject: "org-request-test".into(),
                email: "orgreq@example.com".into(),
                name: Some("Org Req User".into()),
                email_verified: true,
            },
        )
        .await
        .unwrap();

        // Generate a JWT
        let jwt = user
            .generate_jwt(&ctx.config.get_jwt_config().unwrap().secret, 3600)
            .unwrap();

        // Hit /orgs with auth cookie
        let response = request
            .get("/orgs")
            .add_cookie(axum_extra::extract::cookie::Cookie::new("jwt", jwt))
            .await;

        assert_eq!(response.status_code(), 200);
        let body = response.text();
        assert!(
            body.contains("Personal"),
            "Org list should contain personal org"
        );
    })
    .await;
}

/// An Admin must not be able to invite a member at the Owner role — that would
/// bypass the Owner guard in `update_role` and escalate to org control.
#[tokio::test]
#[serial]
async fn admin_cannot_invite_owner() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "inv-owner").await;
        let admin = mk_user(&ctx.db, "inv-admin").await;

        // Owner's personal org; promote `admin` to Admin within it.
        let org = &organizations::Model::find_orgs_for_user(&ctx.db, owner.id)
            .await
            .unwrap()[0];
        org_members::Model::add_member(&ctx.db, org.id, admin.id, OrgRole::Admin)
            .await
            .unwrap();

        let response = request
            .post(&format!("/orgs/{}/members/invite", org.pid))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .form(&[("email", "victim@example.com"), ("role", "owner")])
            .await;

        assert_eq!(
            response.status_code(),
            404,
            "Admin inviting an Owner should be refused"
        );

        // And no invite was persisted.
        let invites =
            fracture_cms::models::org_invites::Model::find_pending_by_org(&ctx.db, org.id)
                .await
                .unwrap();
        assert!(
            invites.is_empty(),
            "no owner-role invite should have been created"
        );
    })
    .await;
}

/// An Admin must not be able to remove an Owner — mirrors the `update_role` guard.
#[tokio::test]
#[serial]
async fn admin_cannot_remove_owner() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "rm-owner").await;
        let admin = mk_user(&ctx.db, "rm-admin").await;
        let other_owner = mk_user(&ctx.db, "rm-owner2").await;

        let org = &organizations::Model::find_orgs_for_user(&ctx.db, owner.id)
            .await
            .unwrap()[0];
        org_members::Model::add_member(&ctx.db, org.id, admin.id, OrgRole::Admin)
            .await
            .unwrap();
        // A second owner exists, so the last-owner guard is NOT what blocks us.
        org_members::Model::add_member(&ctx.db, org.id, other_owner.id, OrgRole::Owner)
            .await
            .unwrap();

        let response = request
            .post(&format!(
                "/orgs/{}/members/{}/remove",
                org.pid, other_owner.pid
            ))
            .add_cookie(jwt_cookie(&ctx, &admin))
            .await;

        assert_eq!(
            response.status_code(),
            404,
            "Admin removing an Owner should be refused"
        );
        assert!(
            org_members::Model::find_membership(&ctx.db, org.id, other_owner.id)
                .await
                .unwrap()
                .is_some(),
            "the owner should still be a member"
        );
    })
    .await;
}
