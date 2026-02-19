use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "organizations",
            &[
                ("id", ColType::PkAuto),
                ("pid", ColType::UuidUniq),
                ("name", ColType::String),
                ("slug", ColType::StringUniq),
                ("is_personal", ColType::Boolean),
            ],
            &[],
        )
        .await?;
        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "organizations").await?;
        Ok(())
    }
}
