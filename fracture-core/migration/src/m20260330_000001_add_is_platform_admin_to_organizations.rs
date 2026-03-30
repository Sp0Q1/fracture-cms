use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Organizations {
    Table,
    IsPlatformAdmin,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .add_column(
                        ColumnDef::new(Organizations::IsPlatformAdmin)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
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
                    .drop_column(Organizations::IsPlatformAdmin)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
