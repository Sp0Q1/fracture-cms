// sea_orm_migration's MigrationTrait uses elided lifetimes in &SchemaManager,
// and the prelude wildcard re-export is required for child modules to use super::*.
#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20220101_000001_users;
mod m20260214_221003_movies;
mod m20260215_000001_add_oidc_to_users;
mod m20260216_000001_add_user_id_to_movies;
mod m20260217_000001_add_pid_to_movies;
mod m20260218_000001_add_session_invalidated_at;
mod m20260220_000001_create_organizations;
mod m20260220_000002_create_org_members;
mod m20260220_000003_create_org_invites;
mod m20260220_000006_drop_movies;
mod m20260330_000001_add_is_platform_admin_to_organizations;
mod m20260330_000002_create_blog_posts;
mod m20260330_000003_create_job_definitions;
mod m20260330_000004_create_job_runs;
mod m20260330_000005_create_job_run_diffs;
mod m20260404_000001_add_settings_to_organizations;
mod m20260406_000001_create_uploads;
mod m20260426_000001_create_resource_assignments;
mod m20260612_000001_add_missing_indexes;
mod m20260612_000003_create_contact_messages;
mod m20260624_000001_rename_is_platform_admin_to_is_staff;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20260214_221003_movies::Migration),
            Box::new(m20260215_000001_add_oidc_to_users::Migration),
            Box::new(m20260216_000001_add_user_id_to_movies::Migration),
            Box::new(m20260217_000001_add_pid_to_movies::Migration),
            Box::new(m20260218_000001_add_session_invalidated_at::Migration),
            Box::new(m20260220_000001_create_organizations::Migration),
            Box::new(m20260220_000002_create_org_members::Migration),
            Box::new(m20260220_000003_create_org_invites::Migration),
            Box::new(m20260220_000006_drop_movies::Migration),
            Box::new(m20260330_000001_add_is_platform_admin_to_organizations::Migration),
            Box::new(m20260330_000002_create_blog_posts::Migration),
            Box::new(m20260330_000003_create_job_definitions::Migration),
            Box::new(m20260330_000004_create_job_runs::Migration),
            Box::new(m20260330_000005_create_job_run_diffs::Migration),
            Box::new(m20260404_000001_add_settings_to_organizations::Migration),
            Box::new(m20260406_000001_create_uploads::Migration),
            Box::new(m20260426_000001_create_resource_assignments::Migration),
            Box::new(m20260612_000001_add_missing_indexes::Migration),
            Box::new(m20260612_000003_create_contact_messages::Migration),
            Box::new(m20260624_000001_rename_is_platform_admin_to_is_staff::Migration),
        ]
    }
}
