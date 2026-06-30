use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum StaffOrgAccess {
    Table,
    Id,
    OrgId,
    UserId,
    FirstAccessedAt,
    LastActiveAt,
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
                    .table(StaffOrgAccess::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StaffOrgAccess::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(StaffOrgAccess::OrgId).integer().not_null())
                    .col(ColumnDef::new(StaffOrgAccess::UserId).integer().not_null())
                    .col(
                        ColumnDef::new(StaffOrgAccess::FirstAccessedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(StaffOrgAccess::LastActiveAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-staff_org_access-org_id")
                            .from(StaffOrgAccess::Table, StaffOrgAccess::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-staff_org_access-user_id")
                            .from(StaffOrgAccess::Table, StaffOrgAccess::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // One record per (org, staff user). The composite unique index also
        // serves the per-org lookup that renders the transparency list.
        manager
            .create_index(
                Index::create()
                    .name("idx-staff_org_access-org_id-user_id")
                    .table(StaffOrgAccess::Table)
                    .col(StaffOrgAccess::OrgId)
                    .col(StaffOrgAccess::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(StaffOrgAccess::Table).to_owned())
            .await?;
        Ok(())
    }
}
