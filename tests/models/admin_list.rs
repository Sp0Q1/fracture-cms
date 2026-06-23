use fracture_cms::app::App;
use fracture_cms::models::users::{self, OidcUserInfo};
use fracture_core::entity_registry::{AdminEntity, ListQuery, UsersEntity};
use loco_rs::testing::prelude::*;
use serial_test::serial;

async fn mk(db: &sea_orm::DatabaseConnection, sub: &str, email: &str, name: &str) {
    users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: sub.to_string(),
            email: email.to_string(),
            name: Some(name.to_string()),
            email_verified: true,
        },
    )
    .await
    .expect("create test user");
}

/// The generic admin changelist: search filters, sorting orders, and the user
/// rows never leak secret columns (password hash / api_key).
#[tokio::test]
#[serial]
async fn admin_user_changelist_search_sort_no_secrets() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    mk(db, "cl-alice", "alice-cl@example.com", "Alice CL").await;
    mk(db, "cl-bob", "bob-cl@example.com", "Bob CL").await;

    // Full list: rows must NOT carry password/api_key.
    let page = UsersEntity.list(db, &ListQuery::default()).await.unwrap();
    assert!(page.total >= 2);
    for row in &page.rows {
        assert!(
            row.get("password").is_none(),
            "password must not be serialized"
        );
        assert!(
            row.get("api_key").is_none(),
            "api_key must not be serialized"
        );
        assert!(row.get("email").is_some());
    }

    // Search narrows to matches only.
    let q = ListQuery {
        q: Some("alice-cl".to_string()),
        ..ListQuery::default()
    };
    let page = UsersEntity.list(db, &q).await.unwrap();
    assert!(!page.rows.is_empty());
    assert!(page
        .rows
        .iter()
        .all(|r| r["email"].as_str().unwrap_or_default().contains("alice-cl")));

    // Sort by email ascending.
    let q = ListQuery {
        sort: Some("email".to_string()),
        desc: false,
        per_page: 200,
        ..ListQuery::default()
    };
    let page = UsersEntity.list(db, &q).await.unwrap();
    let emails: Vec<String> = page
        .rows
        .iter()
        .map(|r| r["email"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut sorted = emails.clone();
    sorted.sort();
    assert_eq!(emails, sorted, "rows should be sorted by email asc");
}
