use super::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum BlogPosts {
    Table,
    Id,
    Pid,
    OrgId,
    AuthorId,
    Title,
    Slug,
    Body,
    BodyHtml,
    Excerpt,
    Status,
    PublishedAt,
    MetaTitle,
    MetaDescription,
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
                    .table(BlogPosts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlogPosts::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BlogPosts::Pid).uuid().not_null())
                    .col(ColumnDef::new(BlogPosts::OrgId).integer().not_null())
                    .col(ColumnDef::new(BlogPosts::AuthorId).integer().not_null())
                    .col(ColumnDef::new(BlogPosts::Title).string().not_null())
                    .col(ColumnDef::new(BlogPosts::Slug).string().not_null())
                    .col(ColumnDef::new(BlogPosts::Body).text().not_null())
                    .col(ColumnDef::new(BlogPosts::BodyHtml).text().not_null())
                    .col(ColumnDef::new(BlogPosts::Excerpt).string().null())
                    .col(
                        ColumnDef::new(BlogPosts::Status)
                            .string()
                            .not_null()
                            .default("draft"),
                    )
                    .col(
                        ColumnDef::new(BlogPosts::PublishedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(BlogPosts::MetaTitle).string().null())
                    .col(ColumnDef::new(BlogPosts::MetaDescription).string().null())
                    .col(
                        ColumnDef::new(BlogPosts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(BlogPosts::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-blog_posts-org_id")
                            .from(BlogPosts::Table, BlogPosts::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-blog_posts-author_id")
                            .from(BlogPosts::Table, BlogPosts::AuthorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-blog_posts-pid")
                    .table(BlogPosts::Table)
                    .col(BlogPosts::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-blog_posts-org_id-slug")
                    .table(BlogPosts::Table)
                    .col(BlogPosts::OrgId)
                    .col(BlogPosts::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BlogPosts::Table).to_owned())
            .await?;
        Ok(())
    }
}
