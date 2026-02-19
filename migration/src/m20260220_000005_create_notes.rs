use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Notes {
    Table,
    Id,
    Pid,
    ProjectId,
    OrgId,
    Title,
    Body,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Projects {
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
                    .table(Notes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Notes::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Notes::Pid).uuid().not_null())
                    .col(ColumnDef::new(Notes::ProjectId).integer().not_null())
                    .col(ColumnDef::new(Notes::OrgId).integer().not_null())
                    .col(ColumnDef::new(Notes::Title).string().not_null())
                    .col(ColumnDef::new(Notes::Body).string().null())
                    .col(
                        ColumnDef::new(Notes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Notes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-notes-project_id")
                            .from(Notes::Table, Notes::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-notes-org_id")
                            .from(Notes::Table, Notes::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-notes-pid")
                    .table(Notes::Table)
                    .col(Notes::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Notes::Table).to_owned())
            .await?;
        Ok(())
    }
}
