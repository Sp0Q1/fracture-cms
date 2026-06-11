use sea_orm::entity::prelude::*;
use sea_orm::TransactionTrait;

pub use super::_entities::org_invites::{ActiveModel, Column, Entity, Model};
use super::_entities::org_members;
use super::org_members::OrgRole;
pub type OrgInvites = Entity;

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
    /// Creates a new invite with a 7-day expiry.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn create_invite(
        db: &DatabaseConnection,
        org_id: i32,
        email: &str,
        role: OrgRole,
        invited_by: i32,
    ) -> Result<Self, DbErr> {
        let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
        ActiveModel {
            org_id: sea_orm::ActiveValue::Set(org_id),
            email: sea_orm::ActiveValue::Set(email.to_string()),
            role: sea_orm::ActiveValue::Set(role.to_string()),
            invited_by_user_id: sea_orm::ActiveValue::Set(invited_by),
            expires_at: sea_orm::ActiveValue::Set(expires_at.into()),
            ..Default::default()
        }
        .insert(db)
        .await
    }

    /// Finds all pending (non-accepted, non-expired) invites for an email.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_pending_by_email(
        db: &DatabaseConnection,
        email: &str,
    ) -> Result<Vec<Self>, DbErr> {
        let now = chrono::Utc::now();
        Entity::find()
            .filter(Column::Email.eq(email))
            .filter(Column::AcceptedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .all(db)
            .await
    }

    /// Finds all pending (non-accepted, non-expired) invites for an org.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_pending_by_org(
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        let now = chrono::Utc::now();
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::AcceptedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .all(db)
            .await
    }

    /// Finds an invite by its public ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Result<Option<Self>, DbErr> {
        let Some(uuid) = Uuid::parse_str(pid).ok() else {
            return Ok(None);
        };
        Entity::find().filter(Column::Pid.eq(uuid)).one(db).await
    }

    /// Accepts an invite: creates the membership and marks the invite as accepted.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn accept_invite(
        db: &DatabaseConnection,
        invite: Self,
        user_id: i32,
    ) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        if invite.accepted_at.is_some() {
            return Err(DbErr::Custom("Invite already accepted".to_string()));
        }
        let now_tz: chrono::DateTime<chrono::FixedOffset> = now.into();
        if invite.expires_at < now_tz {
            return Err(DbErr::Custom("Invite has expired".to_string()));
        }

        let role = OrgRole::from_str_role(&invite.role).unwrap_or(OrgRole::Member);

        // Create the membership and mark the invite accepted in one transaction
        // so a failure between the two steps can't leave a member with a still
        // "pending" invite (or vice versa).
        let txn = db.begin().await?;

        let existing = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(invite.org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&txn)
            .await?;
        if existing.is_none() {
            org_members::ActiveModel {
                org_id: sea_orm::ActiveValue::Set(invite.org_id),
                user_id: sea_orm::ActiveValue::Set(user_id),
                role: sea_orm::ActiveValue::Set(role.to_string()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }

        let mut active: ActiveModel = invite.into();
        active.accepted_at = sea_orm::ActiveValue::Set(Some(now.into()));
        active.update(&txn).await?;

        txn.commit().await?;
        Ok(())
    }
}

impl ActiveModel {}

impl Entity {}

impl super::OrgScoped for Entity {
    fn org_id_column() -> Self::Column {
        Column::OrgId
    }
}
