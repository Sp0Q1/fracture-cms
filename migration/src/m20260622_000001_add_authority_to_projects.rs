//! Reference example: give the demo `projects` resource the columns the
//! capability resolver needs — who created a row (`created_by`) and at what
//! tier (`owner_tier`: "org" for a member-created project, "platform" for a
//! staff/platform-admin-created one). See `docs/ADDING_RESOURCES.md`.

use super::*;

#[derive(DeriveIden)]
enum Projects {
    Table,
    OwnerTier,
    CreatedBy,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite's ALTER TABLE can add columns but not FK constraints, so
        // `created_by` is a plain user-id column (the app sets/reads it); the
        // referential link is enforced in code, consistent with the demo's
        // SQLite-first default.
        // SQLite allows only one operation per ALTER TABLE, so split them.
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .add_column(
                        ColumnDef::new(Projects::OwnerTier)
                            .string()
                            .not_null()
                            .default("org"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .add_column(ColumnDef::new(Projects::CreatedBy).integer().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Projects::OwnerTier)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Projects::CreatedBy)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveMigrationName)]
pub struct Migration;
