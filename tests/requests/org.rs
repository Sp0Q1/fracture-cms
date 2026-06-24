use fracture_cms::{
    app::App,
    models::{
        org_members::{self, OrgRole},
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
        // Create a user and give them membership in an org.
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
        let org = crate::support::owned_org(&ctx.db, "req-list", user.id).await;

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
            body.contains(&org.name),
            "org list should contain the user's org"
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
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
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

        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
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

/// A tenant Owner/Admin cannot change the role of, or remove, a platform-staff
/// member who also belongs to their org — staff are managed by the platform,
/// not the tenant (the UI hides the controls; this guards the raw request too).
#[tokio::test]
#[serial]
async fn staff_members_are_not_manageable_by_org_admins() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "stafflock-owner").await;
        let staff = mk_user(&ctx.db, "stafflock-staff").await;
        let org = crate::support::owned_org(&ctx.db, "stafflock", owner.id).await;
        org_members::Model::add_member(&ctx.db, org.id, staff.id, OrgRole::Member)
            .await
            .unwrap();
        // Make `staff` platform staff via membership in an is_staff org.
        let staff_org = organizations::ActiveModel {
            name: Set("Platform Admin".to_string()),
            slug: Set(format!("platform-admin-{}", staff.id)),
            is_personal: Set(false),
            is_staff: Set(true),
            ..Default::default()
        }
        .insert(&ctx.db)
        .await
        .unwrap();
        org_members::Model::add_member(&ctx.db, staff_org.id, staff.id, OrgRole::Owner)
            .await
            .unwrap();

        // Role change on the staff member is refused.
        let response = request
            .post(&format!("/orgs/{}/members/{}/role", org.pid, staff.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .form(&[("role", "viewer")])
            .await;
        assert_eq!(response.status_code(), 400);

        // Removal of the staff member is refused.
        let response = request
            .post(&format!("/orgs/{}/members/{}/remove", org.pid, staff.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .await;
        assert_eq!(response.status_code(), 400);

        // The staff member is untouched: still a Member of the tenant org.
        let membership = org_members::Model::find_membership(&ctx.db, org.id, staff.id)
            .await
            .unwrap()
            .expect("staff is still a member");
        assert_eq!(membership.role, "member");
    })
    .await;
}

/// Regression: a non-platform-admin member with NO `org_pid` cookie must still
/// resolve an org context via the fallback. A prior `unwrap_or(... return None)`
/// evaluated the bail eagerly and dropped every such member (404 everywhere).
#[tokio::test]
#[serial]
async fn member_resolves_org_context_without_org_pid_cookie() {
    request::<App, _, _>(|request, ctx| async move {
        let user = mk_user(&ctx.db, "no-cookie").await;
        let org = organizations::ActiveModel {
            name: Set("No-Cookie Org".to_string()),
            slug: Set(format!("nocookie-{}", user.id)),
            is_personal: Set(false),
            is_staff: Set(false),
            ..Default::default()
        }
        .insert(&ctx.db)
        .await
        .unwrap();
        org_members::Model::add_member(&ctx.db, org.id, user.id, OrgRole::Member)
            .await
            .unwrap();

        // jwt only — NO org_pid cookie — exercises the fallback path.
        let response = request
            .get("/projects")
            .add_cookie(jwt_cookie(&ctx, &user))
            .await;
        assert_eq!(
            response.status_code(),
            200,
            "a member must resolve an org context without an org_pid cookie"
        );
    })
    .await;
}

/// Deleting an org that is some member's ONLY org would orphan them (no
/// personal orgs exist as a fallback), so it must be refused.
#[tokio::test]
#[serial]
async fn cannot_delete_a_members_last_org() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "del-last").await;
        let org_a = crate::support::owned_org(&ctx.db, "del-a", owner.id).await;

        // org_a is the owner's only org → deletion refused.
        let response = request
            .post(&format!("/orgs/{}/delete", org_a.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .await;
        assert_eq!(
            response.status_code(),
            409,
            "deleting a member's last org must be refused"
        );
        assert!(
            organizations::Model::find_by_pid(&ctx.db, &org_a.pid.to_string())
                .await
                .unwrap()
                .is_some(),
            "the org must still exist after a refused delete"
        );

        // Give the owner a second org; now org_a is no longer anyone's last org.
        let _org_b = crate::support::owned_org(&ctx.db, "del-b", owner.id).await;
        let response = request
            .post(&format!("/orgs/{}/delete", org_a.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .await;
        assert!(
            response.status_code().as_u16() < 400,
            "with another org present, deletion should succeed (got {})",
            response.status_code()
        );
        assert!(
            organizations::Model::find_by_pid(&ctx.db, &org_a.pid.to_string())
                .await
                .unwrap()
                .is_none(),
            "org A should be gone after a successful delete"
        );
    })
    .await;
}
