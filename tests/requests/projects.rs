//! Request tests for the capability-resolver wiring on the demo `projects`
//! resource (see src/authz.rs). Exercises both ownership directions and a
//! per-user grant.

use fracture_cms::{
    app::App,
    models::{
        _entities::projects,
        org_members::{self, OrgRole},
        organizations,
        users::{self, OidcUserInfo},
    },
};
use fracture_core::models::resource_assignments::{self, AssignParams};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serial_test::serial;

async fn mk_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".into(),
            subject: format!("proj-{suffix}"),
            email: format!("proj-{suffix}@example.com"),
            name: Some(format!("Proj {suffix}")),
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

fn org_cookie(org: &organizations::Model) -> axum_extra::extract::cookie::Cookie<'static> {
    axum_extra::extract::cookie::Cookie::new("org_pid", org.pid.to_string())
}

/// A team org with `owner` (Owner) and `member` (Member).
async fn mk_team_org(
    db: &sea_orm::DatabaseConnection,
    suffix: &str,
    owner: &users::Model,
    member: &users::Model,
) -> organizations::Model {
    let org = organizations::ActiveModel {
        name: Set(format!("Team {suffix}")),
        slug: Set(format!("team-{suffix}")),
        is_personal: Set(false),
        is_platform_admin: Set(false),
        settings: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    org_members::Model::add_member(db, org.id, owner.id, OrgRole::Owner)
        .await
        .unwrap();
    org_members::Model::add_member(db, org.id, member.id, OrgRole::Member)
        .await
        .unwrap();
    org
}

async fn mk_project(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
    created_by: i32,
    owner_tier: &str,
) -> projects::Model {
    projects::ActiveModel {
        org_id: Set(org_id),
        title: Set("Demo project".into()),
        owner_tier: Set(owner_tier.into()),
        created_by: Set(Some(created_by)),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

#[tokio::test]
#[serial]
async fn member_can_edit_org_owned_project() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "oo-owner").await;
        let member = mk_user(&ctx.db, "oo-member").await;
        let org = mk_team_org(&ctx.db, "oo", &owner, &member).await;
        let proj = mk_project(&ctx.db, org.id, owner.id, "org").await;

        let resp = request
            .get(&format!("/projects/{}/edit", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(resp.status_code(), 200, "member edits an org-owned project");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn staff_owned_project_caps_member_to_view_and_comment() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "so-owner").await;
        let member = mk_user(&ctx.db, "so-member").await;
        let org = mk_team_org(&ctx.db, "so", &owner, &member).await;
        // Staff/platform-owned project (created_by someone outside the member).
        let proj = mk_project(&ctx.db, org.id, owner.id, "platform").await;

        // Can view...
        let show = request
            .get(&format!("/projects/{}", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(show.status_code(), 200, "member can view a staff project");
        assert!(show.text().contains("Staff-managed"));

        // ...but cannot edit or delete.
        let edit = request
            .get(&format!("/projects/{}/edit", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(
            edit.status_code(),
            404,
            "member cannot edit a staff project"
        );
        let del = request
            .delete(&format!("/projects/{}", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(
            del.status_code(),
            404,
            "member cannot delete a staff project"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn even_org_owner_cannot_edit_staff_owned_project() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "oc-owner").await;
        let member = mk_user(&ctx.db, "oc-member").await;
        let org = mk_team_org(&ctx.db, "oc", &owner, &member).await;
        let proj = mk_project(&ctx.db, org.id, member.id, "platform").await;

        // The downward cap holds even for the org Owner.
        let edit = request
            .get(&format!("/projects/{}/edit", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(
            edit.status_code(),
            404,
            "even Owner can't edit a staff project"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn per_user_grant_lets_member_edit_staff_owned_project() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "g-owner").await;
        let member = mk_user(&ctx.db, "g-member").await;
        let org = mk_team_org(&ctx.db, "g", &owner, &member).await;
        let proj = mk_project(&ctx.db, org.id, owner.id, "platform").await;

        // Without a grant: capped (404 on edit).
        let before = request
            .get(&format!("/projects/{}/edit", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(before.status_code(), 404);

        // Grant this member the `edit` capability on this one project.
        resource_assignments::Model::assign(
            &ctx.db,
            AssignParams {
                user_id: member.id,
                org_id: org.id,
                resource_type: "project",
                resource_id: proj.id,
                role_key: "edit",
                granted_by: Some(owner.id),
                expires_at: None,
            },
        )
        .await
        .unwrap();

        let after = request
            .get(&format!("/projects/{}/edit", proj.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(&org))
            .await;
        assert_eq!(after.status_code(), 200, "a per-user grant lifts the cap");
    })
    .await;
}
