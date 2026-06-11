//! Request tests for the jobs controller: role gates on trigger and create.

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
            subject: format!("jobs-req-{suffix}"),
            email: format!("jobs-req-{suffix}@example.com"),
            name: Some(format!("JobsReq {suffix}")),
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

async fn mk_definition(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
) -> fracture_core::models::_entities::job_definitions::Model {
    fracture_core::models::_entities::job_definitions::ActiveModel {
        org_id: Set(org_id),
        name: Set("stats".to_string()),
        job_type: Set("content_stats".to_string()),
        schedule: Set(None),
        enabled: Set(true),
        config: Set("{}".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

/// A Viewer must not be able to trigger job runs (Member+ action).
#[tokio::test]
#[serial]
async fn viewer_cannot_trigger_job_run() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "trig-owner").await;
        let viewer = mk_user(&ctx.db, "trig-viewer").await;
        let org = &organizations::Model::find_orgs_for_user(&ctx.db, owner.id)
            .await
            .unwrap()[0];
        org_members::Model::add_member(&ctx.db, org.id, viewer.id, OrgRole::Viewer)
            .await
            .unwrap();
        let def = mk_definition(&ctx.db, org.id).await;

        let response = request
            .post(&format!("/jobs/{}/run", def.pid))
            .add_cookie(jwt_cookie(&ctx, &viewer))
            .add_cookie(org_cookie(org))
            .await;
        assert_eq!(response.status_code(), 403, "viewers must not trigger runs");

        let runs = fracture_core::models::job_runs::Model::find_by_definition(&ctx.db, def.id)
            .await
            .unwrap();
        assert!(runs.is_empty(), "no run may be queued by a viewer");
    })
    .await;
}

/// A Member can trigger a run; a second trigger while one is active is a
/// no-op (double-click safety).
#[tokio::test]
#[serial]
async fn member_trigger_queues_one_run() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "trig2-owner").await;
        let member = mk_user(&ctx.db, "trig2-member").await;
        let org = &organizations::Model::find_orgs_for_user(&ctx.db, owner.id)
            .await
            .unwrap()[0];
        org_members::Model::add_member(&ctx.db, org.id, member.id, OrgRole::Member)
            .await
            .unwrap();
        let def = mk_definition(&ctx.db, org.id).await;

        for _ in 0..2 {
            let response = request
                .post(&format!("/jobs/{}/run", def.pid))
                .add_cookie(jwt_cookie(&ctx, &member))
                .add_cookie(org_cookie(org))
                .await;
            assert_eq!(response.status_code(), 303);
        }

        let runs = fracture_core::models::job_runs::Model::find_by_definition(&ctx.db, def.id)
            .await
            .unwrap();
        assert_eq!(runs.len(), 1, "active run must absorb repeat triggers");
        assert_eq!(runs[0].status, "queued");
    })
    .await;
}

/// Only Admins may create job definitions, and unknown job types are refused.
#[tokio::test]
#[serial]
async fn member_cannot_create_definition_and_unknown_type_rejected() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "create-owner").await;
        let member = mk_user(&ctx.db, "create-member").await;
        let org = &organizations::Model::find_orgs_for_user(&ctx.db, owner.id)
            .await
            .unwrap()[0];
        org_members::Model::add_member(&ctx.db, org.id, member.id, OrgRole::Member)
            .await
            .unwrap();

        // Member: refused by the role gate.
        let response = request
            .post("/jobs")
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(org))
            .form(&[("name", "x"), ("job_type", "content_stats")])
            .await;
        assert_eq!(response.status_code(), 403);

        // Owner with an unregistered job type: refused by validation.
        let response = request
            .post("/jobs")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[("name", "x"), ("job_type", "definitely_not_registered")])
            .await;
        assert_eq!(response.status_code(), 400);

        // Owner with a registered type and valid schedule: created.
        let response = request
            .post("/jobs")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[
                ("name", "nightly stats"),
                ("job_type", "content_stats"),
                ("schedule", "0 0 * * * *"),
            ])
            .await;
        assert_eq!(response.status_code(), 303);

        let defs = fracture_core::models::job_definitions::Model::find_all_by_org(&ctx.db, org.id)
            .await
            .unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "nightly stats");
        assert_eq!(defs[0].schedule.as_deref(), Some("0 0 * * * *"));
    })
    .await;
}
