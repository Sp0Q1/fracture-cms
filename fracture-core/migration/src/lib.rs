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
        ]
    }
}
