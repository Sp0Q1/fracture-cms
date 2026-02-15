use chrono::{offset::Local, Duration};
use fracture_cms::{
    app::App,
    models::users::{self, Model, OidcUserInfo, RegisterParams},
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
async fn can_create_with_password() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");

    let params = RegisterParams {
        email: "test@framework.com".to_string(),
        password: "1234".to_string(),
        name: "framework".to_string(),
    };

    let res = Model::create_with_password(&boot.app_context.db, &params).await;

    insta::with_settings!({
        filters => cleanup_user_model()
    }, {
        assert_debug_snapshot!(res);
    });
}
#[tokio::test]
#[serial]
async fn handle_create_with_password_with_duplicate() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let new_user = Model::create_with_password(
        &boot.app_context.db,
        &RegisterParams {
            email: "user1@example.com".to_string(),
            password: "1234".to_string(),
            name: "framework".to_string(),
        },
    )
    .await;

    assert_debug_snapshot!(new_user);
}

#[tokio::test]
#[serial]
async fn can_find_by_email() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let existing_user = Model::find_by_email(&boot.app_context.db, "user1@example.com").await;
    let non_existing_user_results =
        Model::find_by_email(&boot.app_context.db, "un@existing-email.com").await;

    assert_debug_snapshot!(existing_user);
    assert_debug_snapshot!(non_existing_user_results);
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
async fn can_verification_token() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID");

    assert!(
        user.email_verification_sent_at.is_none(),
        "Expected no email verification sent timestamp"
    );
    assert!(
        user.email_verification_token.is_none(),
        "Expected no email verification token"
    );

    let result = user
        .into_active_model()
        .set_email_verification_sent(&boot.app_context.db)
        .await;

    assert!(result.is_ok(), "Failed to set email verification sent");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID after setting verification sent");

    assert!(
        user.email_verification_sent_at.is_some(),
        "Expected email verification sent timestamp to be present"
    );
    assert!(
        user.email_verification_token.is_some(),
        "Expected email verification token to be present"
    );
}

#[tokio::test]
#[serial]
async fn can_set_forgot_password_sent() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID");

    assert!(
        user.reset_sent_at.is_none(),
        "Expected no reset sent timestamp"
    );
    assert!(user.reset_token.is_none(), "Expected no reset token");

    let result = user
        .into_active_model()
        .set_forgot_password_sent(&boot.app_context.db)
        .await;

    assert!(result.is_ok(), "Failed to set forgot password sent");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID after setting forgot password sent");

    assert!(
        user.reset_sent_at.is_some(),
        "Expected reset sent timestamp to be present"
    );
    assert!(
        user.reset_token.is_some(),
        "Expected reset token to be present"
    );
}

#[tokio::test]
#[serial]
async fn can_verified() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID");

    assert!(
        user.email_verified_at.is_none(),
        "Expected email to be unverified"
    );

    let result = user
        .into_active_model()
        .verified(&boot.app_context.db)
        .await;

    assert!(result.is_ok(), "Failed to mark email as verified");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID after verification");

    assert!(
        user.email_verified_at.is_some(),
        "Expected email to be verified"
    );
}

#[tokio::test]
#[serial]
async fn can_reset_password() {
    configure_insta!();

    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID");

    assert!(
        user.verify_password("12341234"),
        "Password verification failed for original password"
    );

    let result = user
        .clone()
        .into_active_model()
        .reset_password(&boot.app_context.db, "new-password")
        .await;

    assert!(result.is_ok(), "Failed to reset password");

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .expect("Failed to find user by PID after password reset");

    assert!(
        user.verify_password("new-password"),
        "Password verification failed for new password"
    );
}

#[tokio::test]
#[serial]
async fn magic_link() {
    let boot = boot_test::<App>().await.unwrap();
    seed::<App>(&boot.app_context).await.unwrap();

    let user = Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
        .await
        .unwrap();

    assert!(
        user.magic_link_token.is_none(),
        "Magic link token should be initially unset"
    );
    assert!(
        user.magic_link_expiration.is_none(),
        "Magic link expiration should be initially unset"
    );

    let create_result = user
        .into_active_model()
        .create_magic_link(&boot.app_context.db)
        .await;

    assert!(
        create_result.is_ok(),
        "Failed to create magic link: {:?}",
        create_result.unwrap_err()
    );

    let updated_user =
        Model::find_by_pid(&boot.app_context.db, "11111111-1111-1111-1111-111111111111")
            .await
            .expect("Failed to refetch user after magic link creation");

    assert!(
        updated_user.magic_link_token.is_some(),
        "Magic link token should be set after creation"
    );

    let magic_link_token = updated_user.magic_link_token.unwrap();
    assert_eq!(
        magic_link_token.len(),
        users::MAGIC_LINK_LENGTH as usize,
        "Magic link token length does not match expected length"
    );

    assert!(
        updated_user.magic_link_expiration.is_some(),
        "Magic link expiration should be set after creation"
    );

    let now = Local::now();
    let should_expired_at = now + Duration::minutes(users::MAGIC_LINK_EXPIRATION_MIN.into());
    let actual_expiration = updated_user.magic_link_expiration.unwrap();

    assert!(
        actual_expiration >= now,
        "Magic link expiration should be in the future or now"
    );

    assert!(
        actual_expiration <= should_expired_at,
        "Magic link expiration exceeds expected maximum expiration time"
    );
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
