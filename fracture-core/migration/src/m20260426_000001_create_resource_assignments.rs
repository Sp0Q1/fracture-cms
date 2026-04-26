use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ResourceAssignments {
    Table,
    Id,
    Pid,
    UserId,
    OrgId,
    ResourceType,
    ResourceId,
    RoleKey,
    GrantedBy,
    GrantedAt,
    ExpiresAt,
    RevokedAt,
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
    // The table has many columns and indexes; the schema definition is naturally long.
    #[allow(clippy::too_many_lines)]
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ResourceAssignments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ResourceAssignments::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ResourceAssignments::Pid).uuid().not_null())
                    .col(
                        ColumnDef::new(ResourceAssignments::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::OrgId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::ResourceType)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::ResourceId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::RoleKey)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::GrantedBy)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::GrantedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::RevokedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ResourceAssignments::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-resource_assignments-user_id")
                            .from(ResourceAssignments::Table, ResourceAssignments::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-resource_assignments-org_id")
                            .from(ResourceAssignments::Table, ResourceAssignments::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-resource_assignments-granted_by")
                            .from(ResourceAssignments::Table, ResourceAssignments::GrantedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // External-facing pid lookup.
        manager
            .create_index(
                Index::create()
                    .name("idx-resource_assignments-pid")
                    .table(ResourceAssignments::Table)
                    .col(ResourceAssignments::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // "Who has access to this resource?" — the most common lookup.
        manager
            .create_index(
                Index::create()
                    .name("idx-resource_assignments-resource")
                    .table(ResourceAssignments::Table)
                    .col(ResourceAssignments::ResourceType)
                    .col(ResourceAssignments::ResourceId)
                    .to_owned(),
            )
            .await?;

        // "What does this user have access to?" — for sidebars / dashboards.
        manager
            .create_index(
                Index::create()
                    .name("idx-resource_assignments-user_type")
                    .table(ResourceAssignments::Table)
                    .col(ResourceAssignments::UserId)
                    .col(ResourceAssignments::ResourceType)
                    .to_owned(),
            )
            .await?;

        // Per-org listing (admins listing all assignments in their org).
        manager
            .create_index(
                Index::create()
                    .name("idx-resource_assignments-org_id")
                    .table(ResourceAssignments::Table)
                    .col(ResourceAssignments::OrgId)
                    .to_owned(),
            )
            .await?;

        // Authorization check path: user × resource × role_key, filtered to active.
        // Application code enforces uniqueness for active rows; SQLite partial-unique
        // semantics are weaker than Postgres, so we keep this index non-unique.
        manager
            .create_index(
                Index::create()
                    .name("idx-resource_assignments-auth_check")
                    .table(ResourceAssignments::Table)
                    .col(ResourceAssignments::UserId)
                    .col(ResourceAssignments::ResourceType)
                    .col(ResourceAssignments::ResourceId)
                    .col(ResourceAssignments::RoleKey)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ResourceAssignments::Table).to_owned())
            .await?;
        Ok(())
    }
}
