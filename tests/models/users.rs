use fracture_cms::{
    app::App,
    models::users::{self, Model, OidcUserInfo},
};
use insta::assert_debug_snapshot;
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue, IntoActiveModel};
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_suffix("users");
        let _guard = settings.bind_to_scope();
    };
}

#[tokio::test]
#[serial]
async fn test_can_validate_model() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");

    let invalid_user = users::ActiveModel {
        name: ActiveValue::set("1".to_string()),
        email: ActiveValue::set("invalid-email".to_string()),
        ..Default::default()
    };

    let res = invalid_user.insert(&boot.app_context.db).await;

    assert_debug_snapshot!(res);
}

#[tokio::test]
#[serial]
async fn can_find_by_pid() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let existing_user =
        Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111").await;
    let non_existing_user_results =
        Model::find_by_pid(&boot.app_context.db, "23232323-2323-2323-2323-232323232323").await;

    assert_debug_snapshot!(existing_user);
    assert_debug_snapshot!(non_existing_user_results);
}

#[tokio::test]
#[serial]
async fn can_create_user_from_oidc() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");

    let info = OidcUserInfo {
        provider: "google".to_string(),
        subject: "google-subject-123".to_string(),
        email: "oidc-new@example.com".to_string(),
        name: Some("OIDC User".to_string()),
    };

    let user = Model::find_or_create_from_oidc(&boot.app_context.db, &info)
        .await
        .expect("Failed to create user from OIDC");

    assert_eq!(user.email, "oidc-new@example.com");
    assert_eq!(user.name, "OIDC User");
    assert_eq!(user.oidc_provider, Some("google".to_string()));
    assert_eq!(user.oidc_subject, Some("google-subject-123".to_string()));
    assert!(
        user.email_verified_at.is_some(),
        "OIDC user should be auto-verified"
    );

    // Calling again should return same user (lookup by provider+subject)
    let user2 = Model::find_or_create_from_oidc(&boot.app_context.db, &info)
        .await
        .expect("Failed to find existing OIDC user");
    assert_eq!(user.id, user2.id);
}

#[tokio::test]
#[serial]
async fn oidc_links_to_existing_verified_email_user() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    // First verify user1's email so OIDC linking is allowed
    let user = Model::find_by_email(&boot.app_context.db, "user1@example.com")
        .await
        .expect("Failed to find user1");
    user.into_active_model()
        .verified(&boot.app_context.db)
        .await
        .expect("Failed to verify user1 email");

    let info = OidcUserInfo {
        provider: "google".to_string(),
        subject: "google-subject-456".to_string(),
        email: "user1@example.com".to_string(),
        name: Some("User One".to_string()),
    };

    let user = Model::find_or_create_from_oidc(&boot.app_context.db, &info)
        .await
        .expect("Failed to link OIDC to existing verified user");

    // Should be the existing user1, now linked
    assert_eq!(user.email, "user1@example.com");
    assert_eq!(user.oidc_provider, Some("google".to_string()));
    assert_eq!(user.oidc_subject, Some("google-subject-456".to_string()));
    assert_eq!(user.id, 1);

    // Calling again should now find by provider+subject (not re-link)
    let user2 = Model::find_or_create_from_oidc(&boot.app_context.db, &info)
        .await
        .expect("Failed to find linked OIDC user on second call");
    assert_eq!(user.id, user2.id);
}

#[tokio::test]
#[serial]
async fn oidc_rejects_linking_to_unverified_email_user() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    // user1 from seed has no email_verified_at set
    let info = OidcUserInfo {
        provider: "google".to_string(),
        subject: "google-subject-456".to_string(),
        email: "user1@example.com".to_string(),
        name: Some("User One".to_string()),
    };

    let result = Model::find_or_create_from_oidc(&boot.app_context.db, &info).await;
    assert!(
        result.is_err(),
        "OIDC linking should be rejected for unverified email accounts"
    );
}

#[tokio::test]
#[serial]
async fn oidc_creates_user_without_name_falls_back_to_email_prefix() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");

    let info = OidcUserInfo {
        provider: "github".to_string(),
        subject: "gh-subject-789".to_string(),
        email: "noname@example.com".to_string(),
        name: None,
    };

    let user = Model::find_or_create_from_oidc(&boot.app_context.db, &info)
        .await
        .expect("Failed to create OIDC user without name");

    assert_eq!(user.email, "noname@example.com");
    assert_eq!(user.name, "noname");
    assert_eq!(user.oidc_provider, Some("github".to_string()));
    assert_eq!(user.oidc_subject, Some("gh-subject-789".to_string()));
    assert!(user.email_verified_at.is_some());
}
