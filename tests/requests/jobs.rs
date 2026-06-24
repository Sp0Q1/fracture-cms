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

/// Sets the global job-permission policy for a test (creating the staff org if
/// needed). The shared test DB persists settings across serial tests, so each
/// permission-sensitive test sets the policy it expects rather than relying on
/// the default.
async fn set_job_policy(
    db: &sea_orm::DatabaseConnection,
    run: fracture_core::jobs::JobAccessLevel,
    manage: fracture_core::jobs::JobAccessLevel,
) {
    if organizations::Model::find_staff_org(db)
        .await
        .unwrap()
        .is_none()
    {
        organizations::ActiveModel {
            name: Set("Platform Admin".to_string()),
            slug: Set("platform-admin".to_string()),
            is_personal: Set(false),
            is_staff: Set(true),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }
    fracture_core::jobs::JobPermissions {
        view: fracture_core::jobs::JobAccessLevel::Viewer,
        run,
        manage,
    }
    .save(db)
    .await
    .unwrap();
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
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        org_members::Model::add_member(&ctx.db, org.id, viewer.id, OrgRole::Viewer)
            .await
            .unwrap();
        // Even with running opened to Members, a Viewer is below it.
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;
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
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        org_members::Model::add_member(&ctx.db, org.id, member.id, OrgRole::Member)
            .await
            .unwrap();
        // Open running to Members for this test.
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;
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
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        org_members::Model::add_member(&ctx.db, org.id, member.id, OrgRole::Member)
            .await
            .unwrap();
        // Managing requires Admin for this test.
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;

        // Member: refused by the policy gate.
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

/// An Admin can edit a definition (rename + reschedule) and delete it; an
/// invalid cron is rejected and leaves the definition unchanged.
#[tokio::test]
#[serial]
async fn admin_can_edit_and_delete_definition() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "edit-owner").await;
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;
        let def = mk_definition(&ctx.db, org.id).await;

        // Rename + reschedule.
        let response = request
            .post(&format!("/jobs/{}/edit", def.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[("name", "renamed"), ("schedule", "0 0 * * * *")])
            .await;
        assert_eq!(response.status_code(), 303);
        let reloaded = fracture_core::models::job_definitions::Model::find_by_pid(
            &ctx.db,
            &def.pid.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reloaded.name, "renamed");
        assert_eq!(reloaded.schedule.as_deref(), Some("0 0 * * * *"));

        // Invalid cron: re-renders the form (200), name unchanged.
        let response = request
            .post(&format!("/jobs/{}/edit", def.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[("name", "should-not-stick"), ("schedule", "not a cron")])
            .await;
        assert_eq!(response.status_code(), 200);
        let reloaded = fracture_core::models::job_definitions::Model::find_by_pid(
            &ctx.db,
            &def.pid.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reloaded.name, "renamed", "invalid edit must not persist");

        // Delete.
        let response = request
            .post(&format!("/jobs/{}/delete", def.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .await;
        assert_eq!(response.status_code(), 303);
        let defs = fracture_core::models::job_definitions::Model::find_all_by_org(&ctx.db, org.id)
            .await
            .unwrap();
        assert!(defs.is_empty(), "definition must be deleted");
    })
    .await;
}

/// A Member must not be able to edit or delete a definition (Admin+ actions).
#[tokio::test]
#[serial]
async fn member_cannot_edit_or_delete_definition() {
    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "ed-owner").await;
        let member = mk_user(&ctx.db, "ed-member").await;
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        org_members::Model::add_member(&ctx.db, org.id, member.id, OrgRole::Member)
            .await
            .unwrap();
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;
        let def = mk_definition(&ctx.db, org.id).await;

        let edit = request
            .post(&format!("/jobs/{}/edit", def.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(org))
            .form(&[("name", "hijacked")])
            .await;
        assert_eq!(edit.status_code(), 403);

        let delete = request
            .post(&format!("/jobs/{}/delete", def.pid))
            .add_cookie(jwt_cookie(&ctx, &member))
            .add_cookie(org_cookie(org))
            .await;
        assert_eq!(delete.status_code(), 403);

        // The definition still exists and is unchanged.
        let reloaded = fracture_core::models::job_definitions::Model::find_by_pid(
            &ctx.db,
            &def.pid.to_string(),
        )
        .await
        .unwrap();
        assert!(reloaded.is_some(), "member must not delete the definition");
    })
    .await;
}

/// The friendly create flow: the per-type form renders the job's declared
/// fields (a project dropdown), and submitting it stores the choice in config.
#[tokio::test]
#[serial]
async fn write_note_friendly_create_flow() {
    use fracture_cms::models::_entities::projects;

    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "wn-owner").await;
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        let project = projects::ActiveModel {
            org_id: Set(org.id),
            title: Set("Reports".to_string()),
            owner_tier: Set("org".to_string()),
            ..Default::default()
        }
        .insert(&ctx.db)
        .await
        .unwrap();
        set_job_policy(
            &ctx.db,
            fracture_core::jobs::JobAccessLevel::Member,
            fracture_core::jobs::JobAccessLevel::Admin,
        )
        .await;

        // The per-type form renders the project dropdown (no raw JSON).
        let form = request
            .get("/jobs/new/write_note")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .await;
        assert_eq!(form.status_code(), 200);
        let body = form.text();
        assert!(
            body.contains("name=\"project_id\""),
            "project dropdown present"
        );
        assert!(body.contains("Reports"), "the org's project is an option");

        // Submitting it stores the chosen project in the definition config.
        let created = request
            .post("/jobs")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[
                ("job_type", "write_note"),
                ("name", "Daily note"),
                ("project_id", project.pid.to_string().as_str()),
                ("schedule", ""),
            ])
            .await;
        assert_eq!(created.status_code(), 303);

        let defs = fracture_core::models::job_definitions::Model::find_all_by_org(&ctx.db, org.id)
            .await
            .unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Daily note");
        assert!(
            defs[0].config.contains(&project.pid.to_string()),
            "config must record the chosen project"
        );
    })
    .await;
}

/// Default-tight policy: with run & manage set to staff-only, even an org Owner
/// (who is not platform staff) cannot trigger or create jobs.
#[tokio::test]
#[serial]
async fn staff_only_policy_blocks_non_staff() {
    use fracture_core::jobs::JobAccessLevel;

    request::<App, _, _>(|request, ctx| async move {
        let owner = mk_user(&ctx.db, "lock-owner").await;
        let org_owned = crate::support::owned_org(&ctx.db, "req", owner.id).await;
        let org = &org_owned;
        set_job_policy(&ctx.db, JobAccessLevel::Staff, JobAccessLevel::Staff).await;
        let def = mk_definition(&ctx.db, org.id).await;

        // Owner cannot run.
        let run = request
            .post(&format!("/jobs/{}/run", def.pid))
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .await;
        assert_eq!(
            run.status_code(),
            403,
            "owner must not run under staff-only"
        );

        // Owner cannot create.
        let create = request
            .post("/jobs")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .add_cookie(org_cookie(org))
            .form(&[("name", "x"), ("job_type", "content_stats")])
            .await;
        assert_eq!(
            create.status_code(),
            403,
            "owner must not create under staff-only"
        );
    })
    .await;
}

/// Staff can view and save the job-permission policy from the admin screen;
/// a non-staff org owner cannot reach it.
#[tokio::test]
#[serial]
async fn staff_configures_job_permissions() {
    use fracture_core::jobs::{JobAccessLevel, JobPermissions};

    request::<App, _, _>(|request, ctx| async move {
        // A staff user (member of an is_staff org).
        let staff = mk_user(&ctx.db, "perm-staff").await;
        let staff_org = organizations::ActiveModel {
            name: Set("Platform Admin".to_string()),
            slug: Set("platform-admin".to_string()),
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

        // A non-staff org owner is refused the settings page.
        let owner = mk_user(&ctx.db, "perm-owner").await;
        let _org = crate::support::owned_org(&ctx.db, "perm", owner.id).await;
        let denied = request
            .get("/admin/job-permissions")
            .add_cookie(jwt_cookie(&ctx, &owner))
            .await;
        assert_eq!(
            denied.status_code(),
            403,
            "non-staff cannot view job permissions"
        );

        // Staff can view and save.
        let view = request
            .get("/admin/job-permissions")
            .add_cookie(jwt_cookie(&ctx, &staff))
            .await;
        assert_eq!(view.status_code(), 200);

        let saved = request
            .post("/admin/job-permissions")
            .add_cookie(jwt_cookie(&ctx, &staff))
            .form(&[("view", "member"), ("run", "admin"), ("manage", "owner")])
            .await;
        assert_eq!(saved.status_code(), 303);

        let perms = JobPermissions::load(&ctx.db).await;
        assert_eq!(perms.view, JobAccessLevel::Member);
        assert_eq!(perms.run, JobAccessLevel::Admin);
        assert_eq!(perms.manage, JobAccessLevel::Owner);
    })
    .await;
}
