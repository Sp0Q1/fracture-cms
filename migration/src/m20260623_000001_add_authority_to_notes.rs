//! Give the demo `notes` resource the columns the capability resolver needs —
//! who created a row (`created_by`) and at what tier (`owner_tier`: "org" for a
//! member-created note, "staff" for a platform-admin-created one). Mirrors the
//! `projects` reference wiring. See `docs/ADDING_RESOURCES.md`.

use super::*;

#[derive(DeriveIden)]
enum Notes {
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
        // SQLite-first default. SQLite allows only one op per ALTER TABLE.
        manager
            .alter_table(
                Table::alter()
                    .table(Notes::Table)
                    .add_column(
                        ColumnDef::new(Notes::OwnerTier)
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
                    .table(Notes::Table)
                    .add_column(ColumnDef::new(Notes::CreatedBy).integer().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Notes::Table)
                    .drop_column(Notes::OwnerTier)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Notes::Table)
                    .drop_column(Notes::CreatedBy)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveMigrationName)]
pub struct Migration;
