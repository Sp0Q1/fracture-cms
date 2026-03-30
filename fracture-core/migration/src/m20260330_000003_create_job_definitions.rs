use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum JobDefinitions {
    Table,
    Id,
    Pid,
    OrgId,
    Name,
    JobType,
    Schedule,
    Enabled,
    Config,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(JobDefinitions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JobDefinitions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(JobDefinitions::Pid).uuid().not_null())
                    .col(ColumnDef::new(JobDefinitions::OrgId).integer().not_null())
                    .col(ColumnDef::new(JobDefinitions::Name).string().not_null())
                    .col(ColumnDef::new(JobDefinitions::JobType).string().not_null())
                    .col(ColumnDef::new(JobDefinitions::Schedule).string().null())
                    .col(
                        ColumnDef::new(JobDefinitions::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(JobDefinitions::Config)
                            .text()
                            .not_null()
                            .default("{}"),
                    )
                    .col(
                        ColumnDef::new(JobDefinitions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(JobDefinitions::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-job_definitions-org_id")
                            .from(JobDefinitions::Table, JobDefinitions::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_definitions-pid")
                    .table(JobDefinitions::Table)
                    .col(JobDefinitions::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_definitions-org_id-name")
                    .table(JobDefinitions::Table)
                    .col(JobDefinitions::OrgId)
                    .col(JobDefinitions::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(JobDefinitions::Table).to_owned())
            .await?;
        Ok(())
    }
}
