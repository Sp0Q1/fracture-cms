use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder,
};
use std::sync::OnceLock;

pub use crate::listing::{paginate_models, ListColumn, ListPage, ListQuery};

/// Trait for entities that appear on the admin dashboard.
#[async_trait]
pub trait AdminEntity: Send + Sync {
    /// Display name shown on the dashboard.
    fn entity_name(&self) -> &'static str;

    /// URL prefix for the admin management page (empty string if no page exists).
    fn url_prefix(&self) -> &'static str;

    /// Short description shown in the management table.
    fn description(&self) -> &'static str;

    /// Label for the action button (defaults to "View").
    fn action_label(&self) -> &'static str {
        "View"
    }

    /// Count all rows for this entity.
    async fn count_all(&self, db: &DatabaseConnection) -> u64;

    /// URL-safe slug for the generic changelist route `/admin/list/{slug}`.
    /// Empty (the default) means this entity has no generic list page.
    fn slug(&self) -> &'static str {
        ""
    }

    /// Columns to show in the changelist (Django's `list_display`).
    fn columns(&self) -> Vec<ListColumn> {
        Vec::new()
    }

    /// Whether this entity is served by the generic changelist.
    fn listable(&self) -> bool {
        !self.slug().is_empty()
    }

    /// Run the changelist query (search + sort + paginate). Implemented by
    /// listable entities; the default rejects.
    async fn list(&self, _db: &DatabaseConnection, _query: &ListQuery) -> Result<ListPage, DbErr> {
        Err(DbErr::Custom("this entity has no generic list view".into()))
    }
}

/// Registry holding all admin-visible entities.
pub struct EntityRegistry {
    entities: Vec<Box<dyn AdminEntity>>,
}

impl EntityRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
        }
    }

    /// Register a new admin entity.
    pub fn register(&mut self, entity: Box<dyn AdminEntity>) {
        self.entities.push(entity);
    }

    /// Return a slice of all registered entities.
    #[must_use]
    pub fn entities(&self) -> &[Box<dyn AdminEntity>] {
        &self.entities
    }

    /// Find a registered entity by its changelist slug.
    #[must_use]
    pub fn find(&self, slug: &str) -> Option<&dyn AdminEntity> {
        self.entities
            .iter()
            .map(AsRef::as_ref)
            .find(|e| !e.slug().is_empty() && e.slug() == slug)
    }
}

impl Default for EntityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static ENTITY_REGISTRY: OnceLock<EntityRegistry> = OnceLock::new();

/// Initialise the global entity registry (uses `get_or_init` for safety).
pub fn init_entity_registry(registry: EntityRegistry) {
    ENTITY_REGISTRY.get_or_init(|| registry);
}

/// Access the global entity registry.
///
/// # Panics
///
/// Panics if called before `init_entity_registry`.
#[must_use]
pub fn entity_registry() -> &'static EntityRegistry {
    ENTITY_REGISTRY
        .get()
        .expect("entity registry not initialised — call init_entity_registry() first")
}

// ---------------------------------------------------------------------------
// Built-in entity implementations
// ---------------------------------------------------------------------------

/// Organizations entity.
pub struct OrgsEntity;

#[async_trait]
impl AdminEntity for OrgsEntity {
    fn entity_name(&self) -> &'static str {
        "Organizations"
    }

    fn url_prefix(&self) -> &'static str {
        "/admin/orgs"
    }

    fn description(&self) -> &'static str {
        "View all organizations and their members"
    }

    fn slug(&self) -> &'static str {
        "orgs"
    }

    fn columns(&self) -> Vec<ListColumn> {
        vec![
            ListColumn::sortable("name", "Name"),
            ListColumn::sortable("slug", "Slug"),
            ListColumn::plain("is_staff", "Staff"),
            ListColumn::plain("is_personal", "Personal"),
        ]
    }

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::organizations::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
    }

    async fn list(&self, db: &DatabaseConnection, q: &ListQuery) -> Result<ListPage, DbErr> {
        use crate::models::_entities::organizations::{Column, Entity};
        let mut query = Entity::find();
        if let Some(s) = &q.q {
            query = query.filter(
                Condition::any()
                    .add(Column::Name.contains(s))
                    .add(Column::Slug.contains(s)),
            );
        }
        let dir = if q.desc { Order::Desc } else { Order::Asc };
        query = match q.sort.as_deref() {
            Some("slug") => query.order_by(Column::Slug, dir),
            Some("name") => query.order_by(Column::Name, dir),
            _ => query.order_by(Column::Name, Order::Asc),
        };
        paginate_models(db, query, q, self.columns(), |m| {
            serde_json::json!({
                "pid": m.pid.to_string(),
                "name": m.name,
                "slug": m.slug,
                "is_staff": m.is_staff,
                "is_personal": m.is_personal,
            })
        })
        .await
    }
}

/// Users entity (no dedicated admin page yet).
pub struct UsersEntity;

#[async_trait]
impl AdminEntity for UsersEntity {
    fn entity_name(&self) -> &'static str {
        "Users"
    }

    fn url_prefix(&self) -> &'static str {
        ""
    }

    fn description(&self) -> &'static str {
        "Registered platform users"
    }

    fn slug(&self) -> &'static str {
        "users"
    }

    fn columns(&self) -> Vec<ListColumn> {
        vec![
            ListColumn::sortable("email", "Email"),
            ListColumn::sortable("name", "Name"),
            ListColumn::plain("verified", "Verified"),
            ListColumn::sortable("created_at", "Joined"),
        ]
    }

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::users::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
    }

    async fn list(&self, db: &DatabaseConnection, q: &ListQuery) -> Result<ListPage, DbErr> {
        use crate::models::_entities::users::{Column, Entity};
        let mut query = Entity::find();
        if let Some(s) = &q.q {
            query = query.filter(
                Condition::any()
                    .add(Column::Email.contains(s))
                    .add(Column::Name.contains(s)),
            );
        }
        let dir = if q.desc { Order::Desc } else { Order::Asc };
        query = match q.sort.as_deref() {
            Some("email") => query.order_by(Column::Email, dir),
            Some("name") => query.order_by(Column::Name, dir),
            Some("created_at") => query.order_by(Column::CreatedAt, dir),
            _ => query.order_by(Column::CreatedAt, Order::Desc),
        };
        // Never serialize the full user model — it carries the password hash and
        // api_key. Project only safe, displayable fields.
        paginate_models(db, query, q, self.columns(), |m| {
            serde_json::json!({
                "pid": m.pid.to_string(),
                "email": m.email,
                "name": m.name,
                "verified": m.email_verified_at.is_some(),
                "created_at": m.created_at.to_string(),
            })
        })
        .await
    }
}

/// Blog posts entity.
pub struct BlogPostsEntity;

#[async_trait]
impl AdminEntity for BlogPostsEntity {
    fn entity_name(&self) -> &'static str {
        "Blog Posts"
    }

    fn url_prefix(&self) -> &'static str {
        "/admin/blog"
    }

    fn description(&self) -> &'static str {
        "Create and manage blog posts"
    }

    fn action_label(&self) -> &'static str {
        "Manage"
    }

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::blog_posts::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
    }
}

/// Job definitions entity.
pub struct JobDefinitionsEntity;

#[async_trait]
impl AdminEntity for JobDefinitionsEntity {
    fn entity_name(&self) -> &'static str {
        "Jobs"
    }

    fn url_prefix(&self) -> &'static str {
        "/admin/jobs"
    }

    fn description(&self) -> &'static str {
        "View all job definitions across organizations"
    }

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::job_definitions::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
    }
}

/// Job runs entity (no dedicated admin page).
pub struct JobRunsEntity;

#[async_trait]
impl AdminEntity for JobRunsEntity {
    fn entity_name(&self) -> &'static str {
        "Job Runs"
    }

    fn url_prefix(&self) -> &'static str {
        ""
    }

    fn description(&self) -> &'static str {
        "Job execution history"
    }

    fn slug(&self) -> &'static str {
        "job-runs"
    }

    fn columns(&self) -> Vec<ListColumn> {
        vec![
            ListColumn::sortable("status", "Status"),
            ListColumn::sortable("started_at", "Started"),
            ListColumn::plain("completed_at", "Completed"),
            ListColumn::plain("error", "Error"),
        ]
    }

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::job_runs::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
    }

    async fn list(&self, db: &DatabaseConnection, q: &ListQuery) -> Result<ListPage, DbErr> {
        use crate::models::_entities::job_runs::{Column, Entity};
        let mut query = Entity::find();
        if let Some(s) = &q.q {
            query = query.filter(Column::Status.contains(s));
        }
        let dir = if q.desc { Order::Desc } else { Order::Asc };
        query = match q.sort.as_deref() {
            Some("status") => query.order_by(Column::Status, dir),
            Some("started_at") => query.order_by(Column::StartedAt, dir),
            _ => query.order_by(Column::CreatedAt, Order::Desc),
        };
        paginate_models(db, query, q, self.columns(), |m| {
            serde_json::json!({
                "pid": m.pid.to_string(),
                "status": m.status,
                "started_at": m.started_at.map(|d| d.to_string()),
                "completed_at": m.completed_at.map(|d| d.to_string()),
                "error": m.error_message,
            })
        })
        .await
    }
}

/// Create a registry pre-populated with the built-in entities.
#[must_use]
pub fn default_entity_registry() -> EntityRegistry {
    let mut registry = EntityRegistry::new();
    registry.register(Box::new(OrgsEntity));
    registry.register(Box::new(UsersEntity));
    registry.register(Box::new(BlogPostsEntity));
    registry.register(Box::new(JobDefinitionsEntity));
    registry.register(Box::new(JobRunsEntity));
    registry
}
