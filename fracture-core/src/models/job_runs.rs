use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::QueryOrder;

pub use super::_entities::job_runs::{ActiveModel, Column, Entity, Model};
pub type JobRuns = Entity;

/// Run statuses that occupy the definition's "active" slot.
const ACTIVE_STATUSES: [&str; 2] = ["queued", "running"];

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4());
        }

        if !insert && this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }

        Ok(this)
    }
}

impl Model {
    /// Finds a job run by its public ID.
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

    /// Returns all runs for a job definition, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_definition(
        db: &DatabaseConnection,
        definition_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .order_by_desc(Column::CreatedAt)
            .all(db)
            .await
    }

    /// Returns the latest completed run for a job definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_latest_completed_by_definition(
        db: &DatabaseConnection,
        definition_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .filter(Column::Status.eq("completed"))
            .order_by_desc(Column::CompletedAt)
            .one(db)
            .await
    }

    /// Creates a new queued job run for the given definition and org.
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert fails.
    pub async fn create_queued(
        db: &DatabaseConnection,
        definition_id: i32,
        org_id: i32,
    ) -> Result<Self, DbErr> {
        ActiveModel {
            job_definition_id: sea_orm::ActiveValue::Set(definition_id),
            org_id: sea_orm::ActiveValue::Set(org_id),
            status: sea_orm::ActiveValue::Set("queued".to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
    }

    /// Returns the most recent run (any status) for a definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_latest_by_definition(
        db: &DatabaseConnection,
        definition_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .order_by_desc(Column::CreatedAt)
            .one(db)
            .await
    }

    /// Returns true if the definition has a queued or running run.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn has_active_run(
        db: &DatabaseConnection,
        definition_id: i32,
    ) -> Result<bool, DbErr> {
        let count = Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .filter(Column::Status.is_in(ACTIVE_STATUSES))
            .count(db)
            .await?;
        Ok(count > 0)
    }

    /// Atomically claims the oldest queued run, transitioning it to
    /// `running` and stamping `started_at`. Returns `None` when nothing is
    /// queued. The compare-and-swap UPDATE (`WHERE id = ? AND status =
    /// 'queued'`) makes concurrent claimants safe: only one sees
    /// `rows_affected == 1`; losers move on to the next queued run.
    ///
    /// # Errors
    ///
    /// Returns an error if a database query fails.
    pub async fn claim_oldest_queued(db: &DatabaseConnection) -> Result<Option<Self>, DbErr> {
        loop {
            let Some(candidate) = Entity::find()
                .filter(Column::Status.eq("queued"))
                .order_by_asc(Column::CreatedAt)
                .one(db)
                .await?
            else {
                return Ok(None);
            };
            let now: DateTimeWithTimeZone = chrono::Utc::now().into();
            let result = Entity::update_many()
                .col_expr(Column::Status, Expr::value("running"))
                .col_expr(Column::StartedAt, Expr::value(now))
                .col_expr(Column::UpdatedAt, Expr::value(now))
                .filter(Column::Id.eq(candidate.id))
                .filter(Column::Status.eq("queued"))
                .exec(db)
                .await?;
            if result.rows_affected == 1 {
                return Entity::find_by_id(candidate.id).one(db).await;
            }
        }
    }

    /// Marks a run completed, recording the JSON result summary.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn mark_completed(
        db: &DatabaseConnection,
        run: Self,
        summary: &serde_json::Value,
    ) -> Result<Self, DbErr> {
        let mut active: ActiveModel = run.into();
        active.status = sea_orm::ActiveValue::Set("completed".to_string());
        active.completed_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now().into()));
        active.result_summary = sea_orm::ActiveValue::Set(Some(summary.to_string()));
        active.update(db).await
    }

    /// Marks a run failed, recording the error message.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn mark_failed(
        db: &DatabaseConnection,
        run: Self,
        error: &str,
    ) -> Result<Self, DbErr> {
        let mut active: ActiveModel = run.into();
        active.status = sea_orm::ActiveValue::Set("failed".to_string());
        active.completed_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now().into()));
        active.error_message = sea_orm::ActiveValue::Set(Some(error.to_string()));
        active.update(db).await
    }
}

impl ActiveModel {}

impl Entity {}

impl super::OrgScoped for Entity {
    fn org_id_column() -> Self::Column {
        Column::OrgId
    }
}
