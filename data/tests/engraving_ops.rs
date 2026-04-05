use data::sparks::types::*;
use data::sparks::engraving_repo;

#[sqlx::test]
async fn test_upsert_and_get(pool: sqlx::SqlitePool) {
    let eng = engraving_repo::upsert(
        &pool,
        NewEngraving {
            key: "auth_pattern".to_string(),
            workshop_id: "ws-eng".to_string(),
            value: "JWT middleware".to_string(),
            author: Some("agent-1".to_string()),
        },
    )
    .await
    .unwrap();

    assert_eq!(eng.key, "auth_pattern");
    assert_eq!(eng.value, "JWT middleware");

    // Upsert with new value
    let updated = engraving_repo::upsert(
        &pool,
        NewEngraving {
            key: "auth_pattern".to_string(),
            workshop_id: "ws-eng".to_string(),
            value: "OAuth2 middleware".to_string(),
            author: Some("agent-2".to_string()),
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.value, "OAuth2 middleware");

    // Get should return updated value
    let fetched = engraving_repo::get(&pool, "auth_pattern", "ws-eng")
        .await
        .unwrap();
    assert_eq!(fetched.value, "OAuth2 middleware");
}

#[sqlx::test]
async fn test_workshop_isolation(pool: sqlx::SqlitePool) {
    engraving_repo::upsert(
        &pool,
        NewEngraving {
            key: "shared_key".to_string(),
            workshop_id: "ws-a".to_string(),
            value: "value-a".to_string(),
            author: None,
        },
    )
    .await
    .unwrap();

    engraving_repo::upsert(
        &pool,
        NewEngraving {
            key: "shared_key".to_string(),
            workshop_id: "ws-b".to_string(),
            value: "value-b".to_string(),
            author: None,
        },
    )
    .await
    .unwrap();

    let a = engraving_repo::get(&pool, "shared_key", "ws-a").await.unwrap();
    let b = engraving_repo::get(&pool, "shared_key", "ws-b").await.unwrap();

    assert_eq!(a.value, "value-a");
    assert_eq!(b.value, "value-b");
}

#[sqlx::test]
async fn test_delete(pool: sqlx::SqlitePool) {
    engraving_repo::upsert(
        &pool,
        NewEngraving {
            key: "temp".to_string(),
            workshop_id: "ws-eng".to_string(),
            value: "val".to_string(),
            author: None,
        },
    )
    .await
    .unwrap();

    engraving_repo::delete(&pool, "temp", "ws-eng").await.unwrap();

    let result = engraving_repo::get(&pool, "temp", "ws-eng").await;
    assert!(result.is_err());
}

#[sqlx::test]
async fn test_list_for_workshop(pool: sqlx::SqlitePool) {
    for i in 0..3 {
        engraving_repo::upsert(
            &pool,
            NewEngraving {
                key: format!("key_{i}"),
                workshop_id: "ws-eng".to_string(),
                value: format!("val_{i}"),
                author: None,
            },
        )
        .await
        .unwrap();
    }

    let list = engraving_repo::list_for_workshop(&pool, "ws-eng")
        .await
        .unwrap();
    assert_eq!(list.len(), 3);
}
