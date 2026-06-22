//! Rename `organizations.is_platform_admin` to `is_staff` so the actor/owner
//! vocabulary is consistent ("org" vs "staff" everywhere). Behaviour is
//! unchanged — this flag still marks the org that confers staff (cross-tenant
//! operator) standing.

use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Organizations {
    Table,
    IsPlatformAdmin,
    IsStaff,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .rename_column(Organizations::IsPlatformAdmin, Organizations::IsStaff)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .rename_column(Organizations::IsStaff, Organizations::IsPlatformAdmin)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
