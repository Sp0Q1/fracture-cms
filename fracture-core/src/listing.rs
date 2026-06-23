//! Reusable list/changelist primitives shared by every table in the app —
//! the admin changelist *and* org-scoped resource lists (projects, notes, …).
//!
//! A controller parses a [`ListQuery`] from the request, builds an org- or
//! staff-scoped `SeaORM` query (applying search + allow-listed sorting), and
//! hands it to [`paginate_models`] to get a [`ListPage`]. Every list then
//! renders through the shared `partials/list_table.html`, so sorting,
//! searching, and pagination work identically everywhere.

use std::collections::HashMap;

use sea_orm::{DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QuerySelect, Select};

/// A column in a list table (Django's `list_display` entry).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListColumn {
    /// Key into the row JSON object.
    pub key: &'static str,
    /// Human label shown in the table header.
    pub label: &'static str,
    /// Whether the column header offers sorting.
    pub sortable: bool,
}

impl ListColumn {
    /// A sortable column.
    #[must_use]
    pub const fn sortable(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            sortable: true,
        }
    }

    /// A display-only (non-sortable) column.
    #[must_use]
    pub const fn plain(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            sortable: false,
        }
    }
}

/// Parsed list query parameters (search, sort, pagination).
#[derive(Debug, Clone)]
pub struct ListQuery {
    /// Free-text search term (Django's `search_fields`).
    pub q: Option<String>,
    /// Column key to sort by.
    pub sort: Option<String>,
    /// Descending when true.
    pub desc: bool,
    /// 1-based page number.
    pub page: u64,
    /// Rows per page.
    pub per_page: u64,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            q: None,
            sort: None,
            desc: false,
            page: 1,
            per_page: 25,
        }
    }
}

impl ListQuery {
    /// Build from raw `?key=value` query params.
    #[must_use]
    pub fn from_params(params: &HashMap<String, String>) -> Self {
        let mut q = Self::default();
        if let Some(s) = params
            .get("q")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            q.q = Some(s);
        }
        if let Some(s) = params.get("sort").filter(|s| !s.is_empty()) {
            q.sort = Some(s.clone());
        }
        q.desc = params.get("dir").map(String::as_str) == Some("desc");
        if let Some(p) = params.get("page").and_then(|s| s.parse::<u64>().ok()) {
            q.page = p.max(1);
        }
        if let Some(pp) = params.get("per_page").and_then(|s| s.parse::<u64>().ok()) {
            q.per_page = pp.clamp(1, 200);
        }
        q
    }

    /// Zero-based page index for `offset`.
    #[must_use]
    pub const fn page_index(&self) -> u64 {
        self.page.saturating_sub(1)
    }
}

/// A page of list results plus the metadata templates need.
#[derive(Debug, serde::Serialize)]
pub struct ListPage {
    pub columns: Vec<ListColumn>,
    pub rows: Vec<serde_json::Value>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
    pub sort: Option<String>,
    pub desc: bool,
    pub q: Option<String>,
}

/// Paginates a filtered + sorted query into a [`ListPage`].
///
/// Each model is mapped to a row via `row_fn`, so each resource controls
/// exactly which fields — never secrets — reach the template.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn paginate_models<E, F>(
    db: &DatabaseConnection,
    query: Select<E>,
    q: &ListQuery,
    columns: Vec<ListColumn>,
    row_fn: F,
) -> Result<ListPage, DbErr>
where
    E: EntityTrait,
    E::Model: Send + Sync,
    F: Fn(&E::Model) -> serde_json::Value,
{
    let per = q.per_page.max(1);
    let total = query.clone().count(db).await?;
    let total_pages = if total == 0 { 1 } else { total.div_ceil(per) };
    let models = query
        .offset(q.page_index() * per)
        .limit(per)
        .all(db)
        .await?;
    let rows = models.iter().map(&row_fn).collect();
    Ok(ListPage {
        columns,
        rows,
        total,
        page: q.page,
        per_page: per,
        total_pages,
        sort: q.sort.clone(),
        desc: q.desc,
        q: q.q.clone(),
    })
}
