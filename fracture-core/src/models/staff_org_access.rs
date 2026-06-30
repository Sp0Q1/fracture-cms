use sea_orm::sea_query::OnConflict;
use sea_orm::{entity::prelude::*, ActiveValue::Set, QueryOrder};

pub use super::_entities::staff_org_access::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Records that `user_id` (a staff member) has accessed `org_id`.
    ///
    /// Upsert keyed on `(org_id, user_id)`: the first call stamps
    /// `first_accessed_at`; every call (including the first) refreshes
    /// `last_active_at`. Called from the org-context resolution path whenever
    /// staff operate in an org they are not a real member of, so a tenant can
    /// see which staff have actually been in their org and when. Best-effort —
    /// callers treat a failure as non-fatal to the request.
    ///
    /// # Errors
    ///
    /// Returns an error if the database write fails.
    pub async fn record(db: &DatabaseConnection, org_id: i32, user_id: i32) -> Result<(), DbErr> {
        let now: DateTimeWithTimeZone = chrono::Utc::now().into();
        Entity::insert(ActiveModel {
            org_id: Set(org_id),
            user_id: Set(user_id),
            first_accessed_at: Set(now),
            last_active_at: Set(now),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([Column::OrgId, Column::UserId])
                .update_column(Column::LastActiveAt)
                .to_owned(),
        )
        .exec(db)
        .await?;
        Ok(())
    }

    /// Returns the staff-access records for `org_id`, each paired with the
    /// staff user, oldest first. Used to render the per-org transparency list.
    ///
    /// # Errors
    ///
    /// Returns an error if a database query fails.
    pub async fn find_for_org_with_users(
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<Vec<(Self, super::users::Model)>, DbErr> {
        let records = Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by_asc(Column::FirstAccessedAt)
            .all(db)
            .await?;
        let user_ids: Vec<i32> = records.iter().map(|r| r.user_id).collect();
        let mut users_by_id: std::collections::HashMap<i32, super::users::Model> =
            super::_entities::users::Entity::find()
                .filter(super::_entities::users::Column::Id.is_in(user_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|u| (u.id, u))
                .collect();
        Ok(records
            .into_iter()
            .filter_map(|r| users_by_id.remove(&r.user_id).map(|u| (r, u)))
            .collect())
    }
}
