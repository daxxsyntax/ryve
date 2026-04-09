// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Typed CRUD for the Workgraph `releases` / `release_epics` tables.
//!
//! Spark ryve-d5032784 [sp-2a82fee7]: this is the foundation every later
//! release-planning spark depends on.

use sqlx::SqlitePool;

use super::error::SparksError;
use super::types::*;

/// Fetch a single release by id.
pub async fn get(pool: &SqlitePool, id: &str) -> Result<Release, SparksError> {
    sqlx::query_as::<_, Release>("SELECT * FROM releases WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SparksError::NotFound(format!("release {id}")))
}

/// List releases, optionally filtered to a set of statuses. An empty filter
/// returns everything, ordered newest-first.
pub async fn list(
    pool: &SqlitePool,
    statuses: Option<Vec<ReleaseStatus>>,
) -> Result<Vec<Release>, SparksError> {
    let mut sql = String::from("SELECT * FROM releases");
    let mut bindings: Vec<String> = Vec::new();

    if let Some(ss) = statuses.filter(|s| !s.is_empty()) {
        let placeholders = ss.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" WHERE status IN ({placeholders})"));
        for s in ss {
            bindings.push(s.as_str().to_string());
        }
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut q = sqlx::query_as::<_, Release>(&sql);
    for b in &bindings {
        q = q.bind(b);
    }
    Ok(q.fetch_all(pool).await?)
}

/// List the spark ids that are members of `release_id`, in the order they
/// were added.
pub async fn list_member_epics(
    pool: &SqlitePool,
    release_id: &str,
) -> Result<Vec<String>, SparksError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT spark_id FROM release_epics WHERE release_id = ? ORDER BY added_at ASC",
    )
    .bind(release_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}
