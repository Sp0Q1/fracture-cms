use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ContactMessages {
    Table,
    Id,
    Pid,
    Name,
    Email,
    Message,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ContactMessages::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ContactMessages::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ContactMessages::Pid).uuid().not_null())
                    .col(ColumnDef::new(ContactMessages::Name).string().not_null())
                    .col(ColumnDef::new(ContactMessages::Email).string().not_null())
                    .col(ColumnDef::new(ContactMessages::Message).text().not_null())
                    .col(
                        ColumnDef::new(ContactMessages::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ContactMessages::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-contact_messages-pid")
                    .table(ContactMessages::Table)
                    .col(ContactMessages::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ContactMessages::Table).to_owned())
            .await?;
        Ok(())
    }
}
