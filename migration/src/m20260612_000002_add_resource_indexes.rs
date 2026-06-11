use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Projects {
    Table,
    OrgId,
}

#[derive(DeriveIden)]
enum Notes {
    Table,
    OrgId,
    ProjectId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Every org-scoped query filters by org_id; the demo resources had no
        // index on it (CLAUDE.md requires one for org-owned tables).
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-projects-org_id")
                    .table(Projects::Table)
                    .col(Projects::OrgId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-notes-org_id")
                    .table(Notes::Table)
                    .col(Notes::OrgId)
                    .to_owned(),
            )
            .await?;
        // Notes are always loaded by their parent project.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-notes-project_id")
                    .table(Notes::Table)
                    .col(Notes::ProjectId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-notes-project_id")
                    .table(Notes::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx-notes-org_id")
                    .table(Notes::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx-projects-org_id")
                    .table(Projects::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
