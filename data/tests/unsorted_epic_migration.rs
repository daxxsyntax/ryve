//! Regression test for migration 011_unsorted_epic_reparent.
//!
//! Because `#[sqlx::test]` runs all migrations before the test body executes,
//! migration 011 first runs against an empty DB (no-op). We then seed orphan
//! non-epic sparks directly and re-execute the migration's SQL to exercise
//! the reparenting path. Finally we run it a second time to verify
//! idempotency — no duplicate 'Unsorted' epics, already-reparented sparks
//! left alone.

const MIGRATION_SQL: &str = include_str!("../migrations/011_unsorted_epic_reparent.sql");

async fn run_migration(pool: &sqlx::SqlitePool) {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("migration 011 should apply cleanly");
}

async fn insert_raw_spark(
    pool: &sqlx::SqlitePool,
    id: &str,
    workshop_id: &str,
    spark_type: &str,
    parent_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO sparks (id, title, description, status, priority, spark_type, workshop_id, parent_id, metadata, created_at, updated_at) \
         VALUES (?, ?, '', 'open', 2, ?, ?, ?, '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(spark_type)
    .bind(workshop_id)
    .bind(parent_id)
    .execute(pool)
    .await
    .expect("seed spark insert");
}

#[sqlx::test]
async fn orphan_reparent_creates_unsorted_epic_per_workshop(pool: sqlx::SqlitePool) {
    // Two orphan tasks in ws-a, one orphan bug in ws-b.
    insert_raw_spark(&pool, "sp-a1", "ws-a", "task", None).await;
    insert_raw_spark(&pool, "sp-a2", "ws-a", "task", None).await;
    insert_raw_spark(&pool, "sp-b1", "ws-b", "bug", None).await;

    // A pre-existing epic and a spark already parented under it should be
    // left untouched by the migration.
    insert_raw_spark(&pool, "ep-a", "ws-a", "epic", None).await;
    insert_raw_spark(&pool, "sp-a-parented", "ws-a", "task", Some("ep-a")).await;

    // An orphan epic should also be left alone — the migration only
    // reparents non-epic orphans.
    insert_raw_spark(&pool, "ep-orphan", "ws-a", "epic", None).await;

    run_migration(&pool).await;

    // No orphan non-epic sparks remain anywhere.
    let orphan_non_epic: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE parent_id IS NULL AND spark_type != 'epic'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        orphan_non_epic, 0,
        "all non-epic orphans must be reparented"
    );

    // Exactly one 'Unsorted' epic per affected workshop.
    let ws_a_unsorted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE workshop_id = 'ws-a' AND spark_type = 'epic' AND title = 'Unsorted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ws_a_unsorted, 1,
        "ws-a should have exactly one Unsorted epic"
    );

    let ws_b_unsorted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE workshop_id = 'ws-b' AND spark_type = 'epic' AND title = 'Unsorted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ws_b_unsorted, 1,
        "ws-b should have exactly one Unsorted epic"
    );

    // The Unsorted epic must be P4 and type=epic.
    let (priority, spark_type): (i64, String) =
        sqlx::query_as("SELECT priority, spark_type FROM sparks WHERE id = 'ws-a-unsorted-epic'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(priority, 4);
    assert_eq!(spark_type, "epic");

    // Orphans were reparented under the workshop's Unsorted epic.
    let parent_a1: String = sqlx::query_scalar("SELECT parent_id FROM sparks WHERE id = 'sp-a1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(parent_a1, "ws-a-unsorted-epic");

    let parent_b1: String = sqlx::query_scalar("SELECT parent_id FROM sparks WHERE id = 'sp-b1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(parent_b1, "ws-b-unsorted-epic");

    // Previously-parented spark untouched.
    let parent_existing: String =
        sqlx::query_scalar("SELECT parent_id FROM sparks WHERE id = 'sp-a-parented'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(parent_existing, "ep-a");

    // Pre-existing orphan epic untouched.
    let orphan_epic_parent: Option<String> =
        sqlx::query_scalar("SELECT parent_id FROM sparks WHERE id = 'ep-orphan'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        orphan_epic_parent.is_none(),
        "orphan epics must not be touched"
    );

    // ── Idempotency: second run is a no-op ─────────────────────────
    run_migration(&pool).await;

    let ws_a_unsorted_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE workshop_id = 'ws-a' AND spark_type = 'epic' AND title = 'Unsorted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ws_a_unsorted_after, 1, "rerun must not duplicate epics");

    let orphan_non_epic_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE parent_id IS NULL AND spark_type != 'epic'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphan_non_epic_after, 0);
}

#[sqlx::test]
async fn migration_is_noop_when_no_orphans_exist(pool: sqlx::SqlitePool) {
    // A workshop with only a properly-parented spark — migration should
    // create no Unsorted epic here.
    insert_raw_spark(&pool, "ep-real", "ws-clean", "epic", None).await;
    insert_raw_spark(&pool, "sp-clean", "ws-clean", "task", Some("ep-real")).await;

    run_migration(&pool).await;

    let unsorted_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sparks WHERE spark_type = 'epic' AND title = 'Unsorted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unsorted_count, 0,
        "no Unsorted epic should be created when there are no orphans"
    );
}
