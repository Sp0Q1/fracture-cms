use fracture_cms::{
    app::App,
    models::{
        _entities::movies::{ActiveModel, Entity, Model},
        users,
    },
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel, ModelTrait};
use serial_test::serial;

async fn create_test_user(db: &sea_orm::DatabaseConnection, suffix: &str) -> users::Model {
    users::Model::find_or_create_from_oidc(
        db,
        &users::OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-movie-user-{suffix}"),
            email: format!("movieuser-{suffix}@example.com"),
            name: Some(format!("Movie User {suffix}")),
        },
    )
    .await
    .expect("Failed to create test user")
}

async fn create_movie(db: &sea_orm::DatabaseConnection, title: &str, user_id: i32) -> Model {
    let item = ActiveModel {
        title: Set(Some(title.to_string())),
        user_id: Set(Some(user_id)),
        ..Default::default()
    };
    item.insert(db).await.expect("Failed to insert movie")
}

#[tokio::test]
#[serial]
async fn test_find_by_user() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "find").await;
    create_movie(db, "User Movie 1", user.id).await;
    create_movie(db, "User Movie 2", user.id).await;

    let movies = Model::find_by_user(db, user.id).await;
    assert_eq!(movies.len(), 2);
    assert_eq!(movies[0].title, Some("User Movie 2".to_string()));
    assert_eq!(movies[1].title, Some("User Movie 1".to_string()));
}

#[tokio::test]
#[serial]
async fn test_find_by_id_and_user() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "findid").await;
    let movie = create_movie(db, "Owned Movie", user.id).await;

    let found = Model::find_by_id_and_user(db, movie.id, user.id).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, Some("Owned Movie".to_string()));

    let not_found = Model::find_by_id_and_user(db, movie.id, user.id + 999).await;
    assert!(not_found.is_none());
}

#[tokio::test]
#[serial]
async fn test_movie_sets_user_id_on_insert() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_test_user(db, "insert").await;
    let movie = create_movie(db, "My Movie", user.id).await;

    assert_eq!(movie.user_id, Some(user.id));

    let found = Entity::find_by_id(movie.id).one(db).await.unwrap().unwrap();
    assert_eq!(found.user_id, Some(user.id));
}

/// Verifies complete ownership isolation: user1 cannot list, view, edit, or
/// delete any of user2's movies — and vice versa.
#[tokio::test]
#[serial]
async fn test_cross_user_ownership_isolation() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let alice = create_test_user(db, "alice").await;
    let bob = create_test_user(db, "bob").await;

    let alice_movie = create_movie(db, "Alice's Movie", alice.id).await;
    let bob_movie = create_movie(db, "Bob's Movie", bob.id).await;

    // --- Listing: each user only sees their own movies ---
    let alice_list = Model::find_by_user(db, alice.id).await;
    let bob_list = Model::find_by_user(db, bob.id).await;

    assert_eq!(alice_list.len(), 1);
    assert_eq!(alice_list[0].id, alice_movie.id);
    assert_eq!(bob_list.len(), 1);
    assert_eq!(bob_list[0].id, bob_movie.id);

    // --- Viewing: alice cannot access bob's movie ---
    assert!(Model::find_by_id_and_user(db, bob_movie.id, alice.id)
        .await
        .is_none());
    assert!(Model::find_by_id_and_user(db, alice_movie.id, bob.id)
        .await
        .is_none());

    // --- Editing: ownership gate prevents cross-user updates ---
    // Alice tries to edit Bob's movie — lookup returns None, so she can't
    // obtain the ActiveModel. Meanwhile, a direct edit on Bob's movie by Bob
    // succeeds, proving the gate is the only barrier.
    assert!(Model::find_by_id_and_user(db, bob_movie.id, alice.id)
        .await
        .is_none());
    let bobs_item = Model::find_by_id_and_user(db, bob_movie.id, bob.id)
        .await
        .expect("Bob should be able to access his own movie");
    let mut bobs_active = bobs_item.into_active_model();
    bobs_active.title = Set(Some("Bob's Updated Movie".to_string()));
    bobs_active
        .update(db)
        .await
        .expect("Bob should be able to update his own movie");

    // Verify Bob's update landed and Alice's movie is untouched
    let bob_refreshed = Entity::find_by_id(bob_movie.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bob_refreshed.title, Some("Bob's Updated Movie".to_string()));
    let alice_refreshed = Entity::find_by_id(alice_movie.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alice_refreshed.title, Some("Alice's Movie".to_string()));

    // --- Deletion: ownership gate prevents cross-user deletes ---
    // Alice cannot obtain Bob's movie, so she can't delete it.
    assert!(Model::find_by_id_and_user(db, bob_movie.id, alice.id)
        .await
        .is_none());

    // Bob deletes his own movie — this should succeed.
    let bobs_item = Model::find_by_id_and_user(db, bob_movie.id, bob.id)
        .await
        .expect("Bob should still be able to access his movie");
    bobs_item
        .delete(db)
        .await
        .expect("Bob should be able to delete his own movie");

    // Bob's movie is gone; Alice's is still intact
    assert!(Entity::find_by_id(bob_movie.id)
        .one(db)
        .await
        .unwrap()
        .is_none());
    assert!(Entity::find_by_id(alice_movie.id)
        .one(db)
        .await
        .unwrap()
        .is_some());
}
