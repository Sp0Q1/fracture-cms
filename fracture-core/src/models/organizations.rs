use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, QueryOrder};

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
    /// Ensures the deployment's default org exists and `user_id` is a member.
    ///
    /// New users join one shared default org (named for the client) rather
    /// than each getting a personal org; additional orgs are staff-created.
    /// The org is created on first use if missing; members join at `role`
    /// (configured via `settings.org.default_role`; staff elevate individuals
    /// as needed). Idempotent — an existing member's role is left untouched.
    ///
    /// # Errors
    ///
    /// Returns an error if a database operation fails.
    pub async fn ensure_default_membership(
        db: &DatabaseConnection,
        slug: &str,
        name: &str,
        role: OrgRole,
        user_id: i32,
    ) -> Result<Self, DbErr> {
        let org = match Self::find_by_slug(db, slug).await? {
            Some(o) => o,
            None => {
                ActiveModel {
                    name: sea_orm::ActiveValue::Set(name.to_string()),
                    slug: sea_orm::ActiveValue::Set(slug.to_string()),
                    is_personal: sea_orm::ActiveValue::Set(false),
                    is_platform_admin: sea_orm::ActiveValue::Set(false),
                    settings: sea_orm::ActiveValue::Set(None),
                    ..Default::default()
                }
                .insert(db)
                .await?
            }
        };
        if org_members::Model::find_membership(db, org.id, user_id)
            .await?
            .is_none()
        {
            org_members::ActiveModel {
                org_id: sea_orm::ActiveValue::Set(org.id),
                user_id: sea_orm::ActiveValue::Set(user_id),
                role: sea_orm::ActiveValue::Set(role.to_string()),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
        Ok(org)
    }

    /// Finds an organization by its public ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Result<Option<Self>, DbErr> {
        let Ok(uuid) = Uuid::parse_str(pid) else {
            return Ok(None);
        };
        // Query once by the parsed UUID — the portable pattern used by every
        // other model. Real DB errors propagate instead of being masked as a
        // not-found (the previous text fallback swallowed connection errors and
        // failed on PostgreSQL's uuid/text type check).
        Entity::find().filter(Column::Pid.eq(uuid)).one(db).await
    }

    /// Finds an organization by slug.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> Result<Option<Self>, DbErr> {
        Entity::find().filter(Column::Slug.eq(slug)).one(db).await
    }

    /// Returns true if any member of `org_id` belongs to no other org, i.e.
    /// deleting this org would leave them with no organization.
    ///
    /// # Errors
    ///
    /// Returns an error if a database query fails.
    pub async fn has_member_whose_only_org_is(
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<bool, DbErr> {
        let member_ids: Vec<i32> = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .all(db)
            .await?
            .into_iter()
            .map(|m| m.user_id)
            .collect();
        for uid in member_ids {
            let others = org_members::Entity::find()
                .filter(org_members::Column::UserId.eq(uid))
                .filter(org_members::Column::OrgId.ne(org_id))
                .count(db)
                .await?;
            if others == 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns true if the user is a member of any org with `is_platform_admin`.
    pub async fn is_user_platform_admin(db: &DatabaseConnection, user_id: i32) -> bool {
        Entity::find()
            .inner_join(org_members::Entity)
            .filter(org_members::Column::UserId.eq(user_id))
            .filter(Column::IsPlatformAdmin.eq(true))
            .count(db)
            .await
            .unwrap_or(0)
            > 0
    }

    /// Returns the parsed settings JSON, or an empty object if unset/invalid.
    #[must_use]
    pub fn get_settings(&self) -> serde_json::Value {
        self.settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    }

    /// Returns a single setting value by key, or `None` if missing.
    #[must_use]
    pub fn get_setting(&self, key: &str) -> Option<serde_json::Value> {
        let settings = self.get_settings();
        settings.get(key).cloned()
    }

    /// Sets a single setting key/value and persists to the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails or the settings JSON
    /// is corrupt.
    pub async fn set_setting(
        db: &DatabaseConnection,
        org_id: i32,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), DbErr> {
        let org = Entity::find_by_id(org_id)
            .one(db)
            .await?
            .ok_or(DbErr::RecordNotFound("organization not found".into()))?;
        let mut settings = org.get_settings();
        let obj = settings
            .as_object_mut()
            .ok_or(DbErr::Custom("settings is not a JSON object".into()))?;
        obj.insert(key.to_string(), value);
        let json = serde_json::to_string(&settings).map_err(|e| DbErr::Custom(e.to_string()))?;
        let mut active: ActiveModel = org.into();
        active.settings = sea_orm::ActiveValue::Set(Some(json));
        active.update(db).await?;
        Ok(())
    }

    /// Finds all organizations a user belongs to.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_orgs_for_user(
        db: &DatabaseConnection,
        user_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .inner_join(org_members::Entity)
            .filter(org_members::Column::UserId.eq(user_id))
            .order_by_asc(Column::Name)
            .all(db)
            .await
    }

    /// Finds all organizations visible to a user.
    /// Platform admins see ALL orgs; regular users see only their memberships.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_visible_orgs(
        db: &DatabaseConnection,
        user_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        if Self::is_user_platform_admin(db, user_id).await {
            Entity::find().order_by_asc(Column::Name).all(db).await
        } else {
            Self::find_orgs_for_user(db, user_id).await
        }
    }
}

impl ActiveModel {}

impl Entity {}
