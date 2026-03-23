use sea_orm::entity::prelude::*;
use sea_orm::{QueryOrder, QuerySelect, TransactionTrait};

use super::_entities::org_members;
pub use super::_entities::organizations::{ActiveModel, Column, Entity, Model};
use super::org_members::OrgRole;
pub type Organizations = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4());
        } else if this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }
        Ok(this)
    }
}

impl Model {
    /// Creates a personal organization for a user and adds them as owner.
    ///
    /// # Errors
    ///
    /// Returns an error if the database transaction fails.
    pub async fn create_personal_org(
        db: &DatabaseConnection,
        user: &super::_entities::users::Model,
    ) -> Result<Self, DbErr> {
        let txn = db.begin().await?;

        let slug = format!("personal-{}", user.pid);
        let org = ActiveModel {
            name: sea_orm::ActiveValue::Set(format!("{}'s Personal", user.name)),
            slug: sea_orm::ActiveValue::Set(slug),
            is_personal: sea_orm::ActiveValue::Set(true),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        org_members::ActiveModel {
            org_id: sea_orm::ActiveValue::Set(org.id),
            user_id: sea_orm::ActiveValue::Set(user.id),
            role: sea_orm::ActiveValue::Set(OrgRole::Owner.to_string()),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        txn.commit().await?;
        Ok(org)
    }

    /// Finds an organization by its public ID.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Option<Self> {
        let uuid = Uuid::parse_str(pid).ok()?;
        // Filter with UUID value (matches both binary and text storage),
        // but decode via into_json() to avoid SeaORM UUID text decode issues.
        let row = Entity::find()
            .filter(Column::Pid.eq(uuid))
            .into_json()
            .one(db)
            .await
            .ok()
            .flatten()?;
        Self::from_json_row(&row)
    }

    /// Finds an organization by slug.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> Option<Self> {
        let row = Entity::find()
            .filter(Column::Slug.eq(slug))
            .into_json()
            .one(db)
            .await
            .ok()
            .flatten()?;
        Self::from_json_row(&row)
    }

    /// Converts a JSON row to a Model, handling SQLite UUID text format.
    fn from_json_row(row: &serde_json::Value) -> Option<Self> {
        let created_str = row.get("created_at")?.as_str()?;
        let updated_str = row.get("updated_at")?.as_str()?;
        let created_at =
            chrono::DateTime::parse_from_str(&format!("{created_str} +00:00"), "%Y-%m-%d %H:%M:%S %z")
                .ok()?;
        let updated_at =
            chrono::DateTime::parse_from_str(&format!("{updated_str} +00:00"), "%Y-%m-%d %H:%M:%S %z")
                .ok()?;
        Some(Self {
            id: row.get("id")?.as_i64()? as i32,
            pid: Uuid::parse_str(row.get("pid")?.as_str()?).ok()?,
            name: row.get("name")?.as_str()?.to_string(),
            slug: row.get("slug")?.as_str()?.to_string(),
            is_personal: row.get("is_personal")?.as_bool()?,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        })
    }

    /// Finds all organizations a user belongs to.
    ///
    /// Uses `into_json()` because `SeaORM` cannot decode UUID text columns
    /// (36 bytes) in `SQLite` via the typed `.all()` — it expects 16-byte
    /// binary blobs. `into_json()` handles both formats.
    pub async fn find_orgs_for_user(db: &DatabaseConnection, user_id: i32) -> Vec<Self> {
        let org_ids: Vec<i32> = match org_members::Entity::find()
            .filter(org_members::Column::UserId.eq(user_id))
            .select_only()
            .column(org_members::Column::OrgId)
            .into_tuple()
            .all(db)
            .await
        {
            Ok(ids) => ids,
            Err(_) => return vec![],
        };

        if org_ids.is_empty() {
            return vec![];
        }

        let json_rows: Vec<serde_json::Value> = match Entity::find()
            .filter(Column::Id.is_in(org_ids))
            .order_by_asc(Column::Name)
            .into_json()
            .all(db)
            .await
        {
            Ok(rows) => rows,
            Err(_) => return vec![],
        };

        json_rows
            .into_iter()
            .filter_map(|row| Self::from_json_row(&row))
            .collect()
    }
}

impl ActiveModel {}

impl Entity {}
