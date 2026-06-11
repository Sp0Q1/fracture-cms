use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Users {
    Table,
    Pid,
}

#[derive(DeriveIden)]
enum OrgInvites {
    Table,
    OrgId,
    Email,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // users.pid is the JWT subject resolved on every authenticated request
        // (find_by_pid). It had no index and no uniqueness guarantee — add a
        // unique index so the lookup is O(log n) and the identity column is
        // enforced unique at the DB level.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-users-pid")
                    .table(Users::Table)
                    .col(Users::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // org_invites is filtered by org_id (find_pending_by_org) and by email
        // (find_pending_by_email, run on every OIDC signup); neither was indexed.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-org_invites-org_id")
                    .table(OrgInvites::Table)
                    .col(OrgInvites::OrgId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-org_invites-email")
                    .table(OrgInvites::Table)
                    .col(OrgInvites::Email)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-org_invites-email")
                    .table(OrgInvites::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx-org_invites-org_id")
                    .table(OrgInvites::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx-users-pid")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
