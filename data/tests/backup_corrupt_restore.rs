// SPDX-License-Identifier: AGPL-3.0-or-later

//! Integration test: corrupt-and-restore cycle.
//!
//! Verifies that after corrupting the live `sparks.db` with garbage bytes,
//! restoring from a snapshot brings back all sparks, bonds, and embers with
//! matching IDs and counts.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use data::backup::{restore_snapshot, take_snapshot};
use data::db::open_sparks_db;
use data::ryve_dir::RyveDir;
use data::sparks::types::*;
use data::sparks::{bond_repo, ember_repo, spark_repo};

struct TempWorkshopDir(PathBuf);

impl Drop for TempWorkshopDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

async fn fresh_workshop() -> (TempWorkshopDir, PathBuf, RyveDir, sqlx::SqlitePool) {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let root = std::env::temp_dir().join(format!("ryve-corrupt-restore-{pid}-{id}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let guard = TempWorkshopDir(root.clone());
    let ryve_dir = RyveDir::new(&root);
    ryve_dir.ensure_exists().await.expect("ensure_exists");
    let pool = open_sparks_db(&root).await.expect("open_sparks_db");
    (guard, root, ryve_dir, pool)
}

async fn create_epic(pool: &sqlx::SqlitePool, title: &str) -> Spark {
    spark_repo::create(
        pool,
        NewSpark {
            title: title.to_string(),
            description: String::new(),
            spark_type: SparkType::Epic,
            priority: 2,
            workshop_id: "ws-test".to_string(),
            assignee: None,
            owner: None,
            parent_id: None,
            due_at: None,
            estimated_minutes: None,
            metadata: None,
            risk_level: None,
            scope_boundary: None,
        },
    )
    .await
    .expect("create spark")
}

async fn create_task(pool: &sqlx::SqlitePool, title: &str, parent_id: &str) -> Spark {
    spark_repo::create(
        pool,
        NewSpark {
            title: title.to_string(),
            description: String::new(),
            spark_type: SparkType::Task,
            priority: 2,
            workshop_id: "ws-test".to_string(),
            assignee: None,
            owner: None,
            parent_id: Some(parent_id.to_string()),
            due_at: None,
            estimated_minutes: None,
            metadata: None,
            risk_level: None,
            scope_boundary: None,
        },
    )
    .await
    .expect("create task spark")
}

async fn create_ember(pool: &sqlx::SqlitePool, content: &str) -> Ember {
    ember_repo::create(
        pool,
        NewEmber {
            ember_type: EmberType::Flash,
            content: content.to_string(),
            source_agent: Some("test-agent".to_string()),
            workshop_id: "ws-test".to_string(),
            ttl_seconds: Some(3600),
        },
    )
    .await
    .expect("create ember")
}

#[tokio::test]
async fn corrupt_and_restore_preserves_sparks_bonds_embers() {
    let (_tmp, _root, ryve_dir, pool) = fresh_workshop().await;

    // --- Step 1: Seed data ---
    let epic = create_epic(&pool, "Epic Alpha").await;
    let task_a = create_task(&pool, "Task A", &epic.id).await;
    let task_b = create_task(&pool, "Task B", &epic.id).await;

    let bond = bond_repo::create(&pool, &task_a.id, &task_b.id, BondType::Blocks)
        .await
        .expect("create bond");

    let ember_1 = create_ember(&pool, "signal-one").await;
    let ember_2 = create_ember(&pool, "signal-two").await;

    let spark_ids: Vec<String> = {
        let mut ids = vec![epic.id.clone(), task_a.id.clone(), task_b.id.clone()];
        ids.sort();
        ids
    };
    let ember_ids: Vec<String> = {
        let mut ids = vec![ember_1.id.clone(), ember_2.id.clone()];
        ids.sort();
        ids
    };

    // --- Step 2: Take backup ---
    let snap = take_snapshot(&pool, &ryve_dir).await.expect("snapshot");
    assert!(snap.exists());

    // --- Step 3: Corrupt the live DB by overwriting the header with garbage ---
    pool.close().await;

    let db_path = ryve_dir.sparks_db_path();
    let garbage: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
        .iter()
        .copied()
        .cycle()
        .take(1024)
        .collect();
    std::fs::write(&db_path, &garbage).expect("corrupt db");

    // Verify the DB is actually corrupt — opening it should fail.
    let open_result = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display())).await;
    if let Ok(p) = &open_result {
        let q = sqlx::query("SELECT 1 FROM sparks").fetch_optional(p).await;
        assert!(q.is_err(), "corrupted db should not serve queries");
        p.close().await;
    }

    // --- Step 4: Restore from backup ---
    let outcome = restore_snapshot(&ryve_dir, &snap).await.expect("restore");
    assert_eq!(outcome.restored_db, db_path);

    // --- Step 5: Verify restored data ---
    let pool = open_sparks_db(ryve_dir.root().parent().unwrap())
        .await
        .expect("reopen after restore");

    // Verify spark count and IDs
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM sparks ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("query sparks");
    let restored_spark_ids: Vec<String> = rows.into_iter().map(|r| r.0).collect();
    assert_eq!(restored_spark_ids, spark_ids, "spark IDs must match");

    // Verify bond count and endpoints
    let bonds: Vec<(String, String, String)> =
        sqlx::query_as("SELECT from_id, to_id, bond_type FROM bonds")
            .fetch_all(&pool)
            .await
            .expect("query bonds");
    assert_eq!(bonds.len(), 1, "exactly one bond expected");
    assert_eq!(bonds[0].0, bond.from_id);
    assert_eq!(bonds[0].1, bond.to_id);
    assert_eq!(bonds[0].2, bond.bond_type);

    // Verify ember count and IDs
    let ember_rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM embers ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("query embers");
    let restored_ember_ids: Vec<String> = ember_rows.into_iter().map(|r| r.0).collect();
    assert_eq!(restored_ember_ids, ember_ids, "ember IDs must match");

    pool.close().await;
}
