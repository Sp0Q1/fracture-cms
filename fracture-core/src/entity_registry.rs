use async_trait::async_trait;
use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait};
use std::sync::OnceLock;

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

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::organizations::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
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

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::users::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
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

    async fn count_all(&self, db: &DatabaseConnection) -> u64 {
        crate::models::_entities::job_runs::Entity::find()
            .count(db)
            .await
            .unwrap_or(0)
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
