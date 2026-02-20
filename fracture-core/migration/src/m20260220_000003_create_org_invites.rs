use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OrgInvites {
    Table,
    Id,
    Pid,
    OrgId,
    InvitedByUserId,
    Email,
    Role,
    AcceptedAt,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
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
                    .table(OrgInvites::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OrgInvites::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OrgInvites::Pid).uuid().not_null())
                    .col(ColumnDef::new(OrgInvites::OrgId).integer().not_null())
                    .col(
                        ColumnDef::new(OrgInvites::InvitedByUserId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(OrgInvites::Email).string().not_null())
                    .col(
                        ColumnDef::new(OrgInvites::Role)
                            .string()
                            .not_null()
                            .default("member"),
                    )
                    .col(
                        ColumnDef::new(OrgInvites::AcceptedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(OrgInvites::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OrgInvites::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(OrgInvites::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-org_invites-org_id")
                            .from(OrgInvites::Table, OrgInvites::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-org_invites-invited_by_user_id")
                            .from(OrgInvites::Table, OrgInvites::InvitedByUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-org_invites-pid")
                    .table(OrgInvites::Table)
                    .col(OrgInvites::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OrgInvites::Table).to_owned())
            .await?;
        Ok(())
    }
}
