//! Comments on notes: a demo resource showcasing the capability system's
//! `COMMENT` action and the "edit/delete your own" pattern. Org members and
//! staff comment; authors edit/delete their own, staff edit/delete any.

use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum NoteComments {
    Table,
    Id,
    Pid,
    NoteId,
    OrgId,
    AuthorId,
    Body,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Notes {
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
                    .table(NoteComments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NoteComments::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(NoteComments::Pid).uuid().not_null())
                    .col(ColumnDef::new(NoteComments::NoteId).integer().not_null())
                    .col(ColumnDef::new(NoteComments::OrgId).integer().not_null())
                    // The comment's author. A plain user-id column (the app sets
                    // and reads it), consistent with `created_by` elsewhere.
                    .col(ColumnDef::new(NoteComments::AuthorId).integer().not_null())
                    .col(ColumnDef::new(NoteComments::Body).string().not_null())
                    .col(
                        ColumnDef::new(NoteComments::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(NoteComments::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-note_comments-note_id")
                            .from(NoteComments::Table, NoteComments::NoteId)
                            .to(Notes::Table, Notes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-note_comments-org_id")
                            .from(NoteComments::Table, NoteComments::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-note_comments-pid")
                    .table(NoteComments::Table)
                    .col(NoteComments::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-note_comments-note_id")
                    .table(NoteComments::Table)
                    .col(NoteComments::NoteId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(NoteComments::Table).to_owned())
            .await?;
        Ok(())
    }
}
