use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::prelude::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Movies {
    Table,
    Pid,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Add pid column as nullable first
        manager
            .alter_table(
                Table::alter()
                    .table(Movies::Table)
                    .add_column(ColumnDef::new(Movies::Pid).uuid().null())
                    .to_owned(),
            )
            .await?;

        // 2. Backfill existing rows with unique UUIDs
        let db = manager.get_connection();
        let rows = db
            .query_all(sea_orm::Statement::from_string(
                manager.get_database_backend(),
                "SELECT id FROM movies WHERE pid IS NULL".to_string(),
            ))
            .await?;

        for row in rows {
            let id: i32 = row.try_get("", "id")?;
            let uuid = Uuid::new_v4().to_string();
            db.execute(sea_orm::Statement::from_string(
                manager.get_database_backend(),
                format!("UPDATE movies SET pid = '{uuid}' WHERE id = {id}"),
            ))
            .await?;
        }

        // 3. Add unique index (NOT NULL enforced at app level via before_save,
        //    since SQLite doesn't support ALTER TABLE ... MODIFY COLUMN)
        manager
            .create_index(
                Index::create()
                    .name("idx-movies-pid")
                    .table(Movies::Table)
                    .col(Movies::Pid)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-movies-pid")
                    .table(Movies::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Movies::Table)
                    .drop_column(Movies::Pid)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
