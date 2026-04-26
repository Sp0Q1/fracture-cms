//! The `OrgScoped` trait pins multi-tenant ownership at the type level.
//!
//! The single hardest correctness property of this codebase is that *every*
//! lookup of an org-owned resource is filtered by `org_id`. This trait makes
//! that the easy path: an entity declares which column holds its org id once,
//! and gets safe query helpers in return. Controllers should reach for
//! [`OrgScopedQuery::find_in_org`] (or model-specific `find_by_pid_in_org`
//! helpers built on top of it) rather than constructing filters by hand.
//!
//! ```ignore
//! use fracture_core::models::OrgScopedQuery;
//! use fracture_core::models::_entities::blog_posts;
//!
//! // Compile-time verified that blog_posts is org-scoped.
//! let posts = blog_posts::Entity::find_in_org(org_id).all(&db).await?;
//! ```
//!
//! Implementations live alongside each entity's domain model file
//! (`models/<resource>.rs`), so the column reference and the query helpers
//! stay in one place.

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select};

/// An entity that has a foreign key column referring to `organizations.id`.
///
/// Implementing this trait is the sanctioned way to declare "this resource
/// belongs to an organization". It enables the blanket [`OrgScopedQuery`]
/// helpers and signals to reviewers that all queries must be org-scoped.
pub trait OrgScoped: EntityTrait {
    /// The column on this entity that holds the owning `org_id`.
    fn org_id_column() -> Self::Column;
}

/// Provided query helpers for any entity that implements [`OrgScoped`].
///
/// This trait is sealed by the blanket impl below — downstream crates do
/// not implement it directly; they get it for free by implementing
/// [`OrgScoped`].
pub trait OrgScopedQuery: EntityTrait {
    /// All rows belonging to a given organization.
    ///
    /// Combine with `.filter(...)`, `.order_by(...)`, etc. as usual.
    fn find_in_org(org_id: i32) -> Select<Self>;
}

impl<T> OrgScopedQuery for T
where
    T: OrgScoped + EntityTrait,
{
    fn find_in_org(org_id: i32) -> Select<Self> {
        Self::find().filter(Self::org_id_column().eq(org_id))
    }
}
