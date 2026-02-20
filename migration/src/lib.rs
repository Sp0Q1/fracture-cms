#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20260220_000004_create_projects;
mod m20260220_000005_create_notes;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        fracture_core_migration::Migrator::migrations()
            .into_iter()
            .chain(vec![
                Box::new(m20260220_000004_create_projects::Migration) as Box<dyn MigrationTrait>,
                Box::new(m20260220_000005_create_notes::Migration),
            ])
            .collect()
    }
}
