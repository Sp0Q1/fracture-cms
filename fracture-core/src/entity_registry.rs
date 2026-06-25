use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder,
};
use std::collections::HashMap;
use std::sync::OnceLock;

pub use crate::listing::{paginate_models, FieldKind, FormField, ListColumn, ListPage, ListQuery};

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

    // -- Generic CRUD (Django's add / change / delete views) ----------------
    //
    // An entity opts into the generic forms by returning a non-empty
    // `form_fields()` and implementing `load`/`create`/`update`/`delete`.
    // The defaults reject, so a list-only entity stays read-only.

    /// Editable fields for the create/edit form (Django's `fields`).
    /// Empty (the default) means the entity has no generic create/edit form.
    fn form_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    /// Whether this entity exposes generic edit/detail forms.
    fn editable(&self) -> bool {
        !self.form_fields().is_empty()
    }

    /// Whether the generic "Add" form is offered. Defaults to [`editable`],
    /// but an entity whose creation needs more than the form (e.g. assigning
    /// an owner through a dedicated flow) can return false to hide it.
    fn creatable(&self) -> bool {
        self.editable()
    }

    /// Load a single row by pid for the detail page and edit prefill.
    /// Returns the field values as a flat JSON object (no secrets).
    async fn load(
        &self,
        _db: &DatabaseConnection,
        _pid: &str,
    ) -> Result<Option<serde_json::Value>, DbErr> {
        Ok(None)
    }

    /// Create a row from submitted form values. `actor_user_id` is the staff
    /// member performing the action (so the new row can record ownership).
    async fn create(
        &self,
        _db: &DatabaseConnection,
        _actor_user_id: i32,
        _form: &HashMap<String, String>,
    ) -> Result<(), DbErr> {
        Err(DbErr::Custom("this entity cannot be created here".into()))
    }

    /// Update the row identified by `pid` from submitted form values.
    async fn update(
        &self,
        _db: &DatabaseConnection,
        _pid: &str,
        _form: &HashMap<String, String>,
    ) -> Result<(), DbErr> {
        Err(DbErr::Custom("this entity cannot be edited here".into()))
    }

    /// Delete the row identified by `pid`. Implementations enforce their own
    /// invariants (e.g. refuse to delete the last org) and return a
    /// `DbErr::Custom` message the controller surfaces to the user.
    async fn delete(&self, _db: &DatabaseConnection, _pid: &str) -> Result<(), DbErr> {
        Err(DbErr::Custom("this entity cannot be deleted here".into()))
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
        let q = q.clone().with_default_sort("name", false);
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
            _ => query.order_by(Column::Name, dir),
        };
        paginate_models(db, query, &q, self.columns(), |m| {
            serde_json::json!({
                "pid": m.pid.to_string(),
                "name": m.name,
                "slug": m.slug,
                "is_staff": m.is_staff,
                "is_personal": m.is_personal,
                "_url": format!("/admin/list/orgs/{}", m.pid),
            })
        })
        .await
    }

    fn form_fields(&self) -> Vec<FormField> {
        // Only the display name is editable here; the slug is derived (renaming
        // it would break existing references) and the is_staff / is_personal
        // flags are structural, set by dedicated flows.
        vec![FormField::text("name", "Name")
            .with_help("Display name. The URL slug is generated from this.")]
    }

    // Org creation assigns an owner, which the dedicated `/orgs/new` flow does;
    // the generic "Add" form would leave the org ownerless, so hide it.
    fn creatable(&self) -> bool {
        false
    }

    async fn load(
        &self,
        db: &DatabaseConnection,
        pid: &str,
    ) -> Result<Option<serde_json::Value>, DbErr> {
        let Some(org) = crate::models::organizations::Model::find_by_pid(db, pid).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::json!({
            "pid": org.pid.to_string(),
            "name": org.name,
            "slug": org.slug,
            "is_staff": org.is_staff,
            "is_personal": org.is_personal,
        })))
    }

    async fn update(
        &self,
        db: &DatabaseConnection,
        pid: &str,
        form: &HashMap<String, String>,
    ) -> Result<(), DbErr> {
        use crate::models::_entities::organizations::Entity;
        use sea_orm::{ActiveModelTrait, ActiveValue::Set};
        let org = crate::models::organizations::Model::find_by_pid(db, pid)
            .await?
            .ok_or_else(|| DbErr::Custom("organization not found".into()))?;
        let name = form
            .get("name")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DbErr::Custom("Name is required.".into()))?
            .to_string();
        let mut active: <Entity as EntityTrait>::ActiveModel = org.into();
        active.name = Set(name);
        active.update(db).await?;
        Ok(())
    }

    async fn delete(&self, db: &DatabaseConnection, pid: &str) -> Result<(), DbErr> {
        use crate::models::organizations::Model;
        use sea_orm::ModelTrait;
        let org = Model::find_by_pid(db, pid)
            .await?
            .ok_or_else(|| DbErr::Custom("organization not found".into()))?;
        if org.is_staff {
            return Err(DbErr::Custom(
                "Cannot delete the staff organization.".into(),
            ));
        }
        if org.is_personal {
            return Err(DbErr::Custom(
                "Cannot delete a personal organization.".into(),
            ));
        }
        if Model::has_member_whose_only_org_is(db, org.id).await? {
            return Err(DbErr::Custom(
                "Cannot delete: a member has no other organization.".into(),
            ));
        }
        org.delete(db).await?;
        Ok(())
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
        let q = q.clone().with_default_sort("created_at", true);
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
            _ => query.order_by(Column::CreatedAt, dir),
        };
        // Never serialize the full user model — it carries the password hash and
        // api_key. Project only safe, displayable fields.
        paginate_models(db, query, &q, self.columns(), |m| {
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
        let q = q.clone().with_default_sort("started_at", true);
        let mut query = Entity::find();
        if let Some(s) = &q.q {
            query = query.filter(Column::Status.contains(s));
        }
        let dir = if q.desc { Order::Desc } else { Order::Asc };
        query = match q.sort.as_deref() {
            Some("status") => query.order_by(Column::Status, dir),
            _ => query.order_by(Column::StartedAt, dir),
        };
        paginate_models(db, query, &q, self.columns(), |m| {
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
