// sea_orm_migration's MigrationTrait uses elided lifetimes in &SchemaManager,
// and the prelude wildcard re-export is required for child modules to use super::*.
#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20260220_000004_create_projects;
mod m20260220_000005_create_notes;
mod m20260612_000002_add_resource_indexes;
mod m20260623_000001_add_authority_to_notes;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        fracture_core_migration::Migrator::migrations()
            .into_iter()
            .chain(vec![
                Box::new(m20260220_000004_create_projects::Migration) as Box<dyn MigrationTrait>,
                Box::new(m20260220_000005_create_notes::Migration),
                Box::new(m20260612_000002_add_resource_indexes::Migration),
                Box::new(m20260623_000001_add_authority_to_notes::Migration),
            ])
            .collect()
    }
}
