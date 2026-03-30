use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum JobRuns {
    Table,
    Id,
    Pid,
    JobDefinitionId,
    OrgId,
    Status,
    StartedAt,
    CompletedAt,
    ErrorMessage,
    ResultSummary,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum JobDefinitions {
    Table,
    Id,
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
                    .table(JobRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JobRuns::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(JobRuns::Pid).uuid().not_null())
                    .col(
                        ColumnDef::new(JobRuns::JobDefinitionId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(JobRuns::OrgId).integer().not_null())
                    .col(
                        ColumnDef::new(JobRuns::Status)
                            .string()
                            .not_null()
                            .default("queued"),
                    )
                    .col(
                        ColumnDef::new(JobRuns::StartedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(JobRuns::CompletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(JobRuns::ErrorMessage).text().null())
                    .col(ColumnDef::new(JobRuns::ResultSummary).text().null())
                    .col(
                        ColumnDef::new(JobRuns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(JobRuns::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-job_runs-job_definition_id")
                            .from(JobRuns::Table, JobRuns::JobDefinitionId)
                            .to(JobDefinitions::Table, JobDefinitions::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-job_runs-org_id")
                            .from(JobRuns::Table, JobRuns::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_runs-pid")
                    .table(JobRuns::Table)
                    .col(JobRuns::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_runs-job_definition_id")
                    .table(JobRuns::Table)
                    .col(JobRuns::JobDefinitionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_runs-org_id-status")
                    .table(JobRuns::Table)
                    .col(JobRuns::OrgId)
                    .col(JobRuns::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(JobRuns::Table).to_owned())
            .await?;
        Ok(())
    }
}
