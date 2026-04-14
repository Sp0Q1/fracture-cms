use comrak::{markdown_to_html, Options};
use loco_rs::config::Config;
use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;

pub use super::_entities::blog_posts::{ActiveModel, Column, Entity, Model};
pub type BlogPosts = Entity;

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

        // Render markdown to HTML when body changes
        if this.body.is_set() {
            let body = this.body.as_ref().clone();
            this.body_html = sea_orm::ActiveValue::Set(render_markdown(&body));
        }

        if !insert && this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }

        Ok(this)
    }
}

/// Renders Markdown to safe HTML using comrak with GFM extensions.
fn render_markdown(input: &str) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.r#unsafe = false;
    markdown_to_html(input, &options)
}

impl Model {
    /// Finds a blog post by its public ID.
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

    /// Finds a published blog post by org and slug.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_published_by_slug(
        db: &DatabaseConnection,
        org_id: i32,
        slug: &str,
    ) -> Result<Option<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::Slug.eq(slug))
            .filter(Column::Status.eq("published"))
            .one(db)
            .await
    }

    /// Returns all published posts for an org, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_published_by_org(
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::Status.eq("published"))
            .order_by_desc(Column::PublishedAt)
            .all(db)
            .await
    }

    /// Returns all posts for an org (any status), newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_all_by_org(db: &DatabaseConnection, org_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by_desc(Column::CreatedAt)
            .all(db)
            .await
    }

    /// Reads the blog org slug from the application config.
    /// Expects `settings.blog.org_slug` in the YAML config.
    pub fn get_blog_org_slug(config: &Config) -> Option<String> {
        config
            .settings
            .as_ref()?
            .get("blog")?
            .get("org_slug")?
            .as_str()
            .map(String::from)
    }
}

impl ActiveModel {}

impl Entity {}
