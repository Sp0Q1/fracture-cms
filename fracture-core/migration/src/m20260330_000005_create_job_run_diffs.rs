use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum JobRunDiffs {
    Table,
    Id,
    JobRunId,
    DiffType,
    EntityKey,
    OldValue,
    NewValue,
    CreatedAt,
}

#[derive(DeriveIden)]
enum JobRuns {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(JobRunDiffs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JobRunDiffs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(JobRunDiffs::JobRunId).integer().not_null())
                    .col(ColumnDef::new(JobRunDiffs::DiffType).string().not_null())
                    .col(ColumnDef::new(JobRunDiffs::EntityKey).string().not_null())
                    .col(ColumnDef::new(JobRunDiffs::OldValue).text().null())
                    .col(ColumnDef::new(JobRunDiffs::NewValue).text().null())
                    .col(
                        ColumnDef::new(JobRunDiffs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-job_run_diffs-job_run_id")
                            .from(JobRunDiffs::Table, JobRunDiffs::JobRunId)
                            .to(JobRuns::Table, JobRuns::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-job_run_diffs-job_run_id")
                    .table(JobRunDiffs::Table)
                    .col(JobRunDiffs::JobRunId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(JobRunDiffs::Table).to_owned())
            .await?;
        Ok(())
    }
}
