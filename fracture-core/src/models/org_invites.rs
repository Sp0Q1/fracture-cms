use sea_orm::entity::prelude::*;

pub use super::_entities::org_invites::{ActiveModel, Column, Entity, Model};
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
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4().to_string());
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
    pub async fn find_pending_by_email(db: &DatabaseConnection, email: &str) -> Vec<Self> {
        let now = chrono::Utc::now();
        Entity::find()
            .filter(Column::Email.eq(email))
            .filter(Column::AcceptedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .all(db)
            .await
            .unwrap_or_default()
    }

    /// Finds all pending (non-accepted, non-expired) invites for an org.
    pub async fn find_pending_by_org(db: &DatabaseConnection, org_id: i32) -> Vec<Self> {
        let now = chrono::Utc::now();
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::AcceptedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .all(db)
            .await
            .unwrap_or_default()
    }

    /// Finds an invite by its public ID.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Option<Self> {
        Uuid::parse_str(pid).ok()?;
        Entity::find()
            .filter(Column::Pid.eq(pid))
            .one(db)
            .await
            .ok()
            .flatten()
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

        // Check if already a member
        let existing = super::org_members::Model::find_membership(db, invite.org_id, user_id).await;
        if existing.is_none() {
            super::org_members::Model::add_member(db, invite.org_id, user_id, role).await?;
        }

        // Mark invite as accepted
        let mut active: ActiveModel = invite.into();
        active.accepted_at = sea_orm::ActiveValue::Set(Some(now.into()));
        active.update(db).await?;
        Ok(())
    }
}

impl ActiveModel {}

impl Entity {}
