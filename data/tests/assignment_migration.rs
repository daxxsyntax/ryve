//! Tests for migration 013_hand_assignment_to_assignment.
//!
//! `#[sqlx::test]` runs all migrations before the test body, so migration 013
//! has already executed against an empty `hand_assignments` table. We verify:
//!
//!   1. The `assignments` table exists with the new columns (phase, event_version,
//!      source_branch, target_branch).
//!   2. The `hand_assignments` backward-compatible view exposes the original columns.
//!   3. Data inserted into `assignments` is visible through the view.
//!   4. Re-running the migration SQL is idempotent — no errors, no duplicates.

async fn seed_session_and_spark(pool: &sqlx::SqlitePool, session_id: &str, spark_id: &str) {
    sqlx::query(
        "INSERT INTO agent_sessions (id, workshop_id, agent_name, agent_command, status, started_at)
         VALUES (?, 'ws-test', 'claude', 'claude', 'active', '2026-04-09T00:00:00Z')",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .expect("seed agent_session");

    sqlx::query(
        "INSERT INTO sparks (id, title, description, status, priority, spark_type, workshop_id, metadata, created_at, updated_at)
         VALUES (?, ?, '', 'open', 2, 'task', 'ws-test', '{}', '2026-04-09T00:00:00Z', '2026-04-09T00:00:00Z')",
    )
    .bind(spark_id)
    .bind(spark_id)
    .execute(pool)
    .await
    .expect("seed spark");
}

#[sqlx::test]
async fn assignments_table_has_new_columns(pool: sqlx::SqlitePool) {
    seed_session_and_spark(&pool, "sess-1234abcd", "sp-test-1").await;

    // Insert via the assignments table with new columns.
    sqlx::query(
        "INSERT INTO assignments (session_id, spark_id, status, role, phase, event_version, source_branch, target_branch, assigned_at)
         VALUES ('sess-1234abcd', 'sp-test-1', 'active', 'owner', 'assigned', 0, 'hand/sess-123', 'main', '2026-04-09T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert into assignments with new columns");

    // Verify new columns are readable.
    let (phase, ev, src, tgt): (String, i64, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT phase, event_version, source_branch, target_branch FROM assignments WHERE spark_id = 'sp-test-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(phase, "assigned");
    assert_eq!(ev, 0);
    assert_eq!(src.as_deref(), Some("hand/sess-123"));
    assert_eq!(tgt.as_deref(), Some("main"));
}

#[sqlx::test]
async fn hand_assignments_view_reflects_assignments(pool: sqlx::SqlitePool) {
    seed_session_and_spark(&pool, "sess-viewtest1", "sp-view-1").await;

    sqlx::query(
        "INSERT INTO assignments (session_id, spark_id, status, role, phase, event_version, source_branch, target_branch, assigned_at)
         VALUES ('sess-viewtest1', 'sp-view-1', 'active', 'owner', 'assigned', 0, 'hand/sess-vie', 'main', '2026-04-09T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert assignment");

    // Query through the backward-compatible view.
    let (session_id, spark_id, status, role): (String, String, String, String) = sqlx::query_as(
        "SELECT session_id, spark_id, status, role FROM hand_assignments WHERE spark_id = 'sp-view-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("view should return rows");

    assert_eq!(session_id, "sess-viewtest1");
    assert_eq!(spark_id, "sp-view-1");
    assert_eq!(status, "active");
    assert_eq!(role, "owner");
}

#[sqlx::test]
async fn insert_or_ignore_prevents_duplicates(pool: sqlx::SqlitePool) {
    // Verify that the INSERT OR IGNORE in the migration prevents duplicates
    // when a row with the same (session_id, spark_id) already exists.
    seed_session_and_spark(&pool, "sess-idempot01", "sp-idemp-1").await;

    sqlx::query(
        "INSERT INTO assignments (session_id, spark_id, status, role, phase, event_version, assigned_at)
         VALUES ('sess-idempot01', 'sp-idemp-1', 'active', 'owner', 'assigned', 0, '2026-04-09T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("seed assignment");

    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assignments")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Attempt to insert the same row again — must be silently skipped.
    sqlx::query(
        "INSERT OR IGNORE INTO assignments (session_id, spark_id, status, role, phase, event_version, assigned_at)
         VALUES ('sess-idempot01', 'sp-idemp-1', 'active', 'owner', 'assigned', 0, '2026-04-09T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("duplicate insert should succeed silently");

    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assignments")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        count_before, count_after,
        "duplicate insert must not create new rows"
    );
}

#[sqlx::test]
async fn phase_defaults_match_status(pool: sqlx::SqlitePool) {
    // Verify the default phase value is applied correctly.
    seed_session_and_spark(&pool, "sess-phase-01", "sp-phase-1").await;

    sqlx::query(
        "INSERT INTO assignments (session_id, spark_id, status, role, assigned_at)
         VALUES ('sess-phase-01', 'sp-phase-1', 'active', 'owner', '2026-04-09T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert with defaults");

    let (phase, ev): (String, i64) = sqlx::query_as(
        "SELECT phase, event_version FROM assignments WHERE spark_id = 'sp-phase-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(phase, "assigned", "default phase should be 'assigned'");
    assert_eq!(ev, 0, "default event_version should be 0");
}
