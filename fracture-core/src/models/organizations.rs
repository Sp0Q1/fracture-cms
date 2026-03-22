use sea_orm::entity::prelude::*;
use sea_orm::{QueryOrder, TransactionTrait};

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
        let uuid = match Uuid::parse_str(pid) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("[ORG] find_by_pid: invalid UUID '{pid}': {e}");
                return None;
            }
        };
        match Entity::find()
            .filter(Column::Pid.eq(uuid))
            .one(db)
            .await
        {
            Ok(result) => {
                eprintln!("[ORG] find_by_pid('{pid}'): found={}", result.is_some());
                result
            }
            Err(e) => {
                eprintln!("[ORG] find_by_pid('{pid}') FAILED: {e}");
                None
            }
        }
    }

    /// Finds an organization by slug.
    pub async fn find_by_slug(db: &DatabaseConnection, slug: &str) -> Option<Self> {
        Entity::find()
            .filter(Column::Slug.eq(slug))
            .one(db)
            .await
            .ok()
            .flatten()
    }

    /// Finds all organizations a user belongs to.
    pub async fn find_orgs_for_user(db: &DatabaseConnection, user_id: i32) -> Vec<Self> {
        eprintln!("[ORG] find_orgs_for_user called for user_id={user_id}");

        // Debug: check if any org_members rows exist for this user
        match org_members::Entity::find()
            .filter(org_members::Column::UserId.eq(user_id))
            .all(db)
            .await
        {
            Ok(members) => eprintln!(
                "[ORG] user_id={user_id} has {} org_member rows: {:?}",
                members.len(),
                members.iter().map(|m| (m.org_id, &m.role)).collect::<Vec<_>>()
            ),
            Err(e) => eprintln!("[ORG] org_members query failed: {e}"),
        }

        match Entity::find()
            .inner_join(org_members::Entity)
            .filter(org_members::Column::UserId.eq(user_id))
            .order_by_asc(Column::Name)
            .all(db)
            .await
        {
            Ok(orgs) => {
                eprintln!("[ORG] find_orgs_for_user returned {} orgs for user_id={user_id}", orgs.len());
                orgs
            }
            Err(e) => {
                eprintln!("[ORG] find_orgs_for_user FAILED for user_id={user_id}: {e}");
                vec![]
            }
        }
    }
}

impl ActiveModel {}

impl Entity {}
