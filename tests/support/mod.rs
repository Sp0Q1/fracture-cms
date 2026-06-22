//! Shared test helpers.

use fracture_cms::models::{
    org_members::{self, OrgRole},
    organizations,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};

/// Creates a fresh org and makes `user_id` its Owner.
///
/// Production no longer auto-creates a personal org per user (new users join
/// the deployment's shared default org as Viewer), so tests that need a user
/// who owns a working org create one explicitly via this helper. The slug
/// includes the user id to stay unique.
pub async fn owned_org(
    db: &DatabaseConnection,
    suffix: &str,
    user_id: i32,
) -> organizations::Model {
    let org = organizations::ActiveModel {
        name: Set(format!("Test Org {suffix}")),
        slug: Set(format!("test-{suffix}-{user_id}")),
        is_personal: Set(false),
        is_staff: Set(false),
        settings: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("create test org");
    org_members::Model::add_member(db, org.id, user_id, OrgRole::Owner)
        .await
        .expect("add owner membership");
    org
}
