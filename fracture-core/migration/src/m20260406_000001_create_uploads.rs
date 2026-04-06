use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Uploads {
    Table,
    Id,
    Pid,
    OrgId,
    UploadedBy,
    OriginalName,
    StoragePath,
    ContentType,
    SizeBytes,
    Visibility,
    ChecksumSha256,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Uploads::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Uploads::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Uploads::Pid).uuid().not_null())
                    .col(ColumnDef::new(Uploads::OrgId).integer().not_null())
                    .col(ColumnDef::new(Uploads::UploadedBy).integer().not_null())
                    .col(
                        ColumnDef::new(Uploads::OriginalName)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Uploads::StoragePath)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Uploads::ContentType)
                            .string_len(127)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Uploads::SizeBytes).big_integer().not_null())
                    .col(
                        ColumnDef::new(Uploads::Visibility)
                            .string_len(16)
                            .not_null()
                            .default("org"),
                    )
                    .col(
                        ColumnDef::new(Uploads::ChecksumSha256)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Uploads::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-uploads-org_id")
                            .from(Uploads::Table, Uploads::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-uploads-uploaded_by")
                            .from(Uploads::Table, Uploads::UploadedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-uploads-pid")
                    .table(Uploads::Table)
                    .col(Uploads::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-uploads-org_id")
                    .table(Uploads::Table)
                    .col(Uploads::OrgId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Uploads::Table).to_owned())
            .await?;
        Ok(())
    }
}
