// SPDX-License-Identifier: AGPL-3.0-or-later

//! Dependency graph operations: cycle detection, hot-spark resolution,
//! and topological ordering for chain alloys.

use std::collections::HashSet;

use petgraph::algo::is_cyclic_directed;
use petgraph::graphmap::DiGraphMap;
use sqlx::SqlitePool;

use super::error::SparksError;
use super::types::*;

/// Check if adding a blocking bond from→to would create a cycle.
pub async fn would_create_cycle(
    pool: &SqlitePool,
    from_id: &str,
    to_id: &str,
) -> Result<bool, SparksError> {
    // Load all existing blocking bonds
    let bonds = sqlx::query_as::<_, Bond>(
        "SELECT * FROM bonds WHERE bond_type IN ('blocks', 'conditional_blocks')",
    )
    .fetch_all(pool)
    .await?;

    let mut graph = DiGraphMap::<&str, ()>::new();

    for bond in &bonds {
        graph.add_edge(bond.from_id.as_str(), bond.to_id.as_str(), ());
    }

    // Tentatively add the proposed edge
    graph.add_edge(from_id, to_id, ());

    Ok(is_cyclic_directed(&graph))
}

/// Find all "hot" sparks — ready to work on.
///
/// A spark is hot when:
/// 1. Status is open or in_progress
/// 2. Not deferred (defer_until is null or in the past)
/// 3. No open blocking bonds (all blockers are closed)
/// 4. Not a child of a deferred parent
///
/// Sorted by priority (P0 first), then created_at.
pub async fn hot_sparks(pool: &SqlitePool, workshop_id: &str) -> Result<Vec<Spark>, SparksError> {
    // Step 1: Find all spark IDs that are blocked by an open spark
    let blocked_ids: HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT DISTINCT b.to_id
         FROM bonds b
         JOIN sparks s ON s.id = b.from_id
         WHERE b.bond_type IN ('blocks', 'conditional_blocks')
           AND s.status != 'closed'",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id,)| id)
    .collect();

    // Step 2: Find deferred spark IDs and their children
    let deferred_ids: HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM sparks
         WHERE defer_until IS NOT NULL
           AND datetime(defer_until) > datetime('now')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id,)| id)
    .collect();

    let deferred_children: HashSet<String> = if deferred_ids.is_empty() {
        HashSet::new()
    } else {
        sqlx::query_as::<_, (String,)>(
            "SELECT id FROM sparks
             WHERE parent_id IS NOT NULL
               AND parent_id IN (SELECT id FROM sparks WHERE defer_until IS NOT NULL AND datetime(defer_until) > datetime('now'))",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect()
    };

    // Step 3: Fetch candidate sparks
    let candidates = sqlx::query_as::<_, Spark>(
        "SELECT * FROM sparks
         WHERE workshop_id = ?
           AND status IN ('open', 'in_progress')
         ORDER BY priority ASC, created_at ASC",
    )
    .bind(workshop_id)
    .fetch_all(pool)
    .await?;

    // Step 4: Filter out blocked and deferred
    let hot: Vec<Spark> = candidates
        .into_iter()
        .filter(|s| {
            !blocked_ids.contains(&s.id)
                && !deferred_ids.contains(&s.id)
                && !deferred_children.contains(&s.id)
        })
        .collect();

    Ok(hot)
}

/// Topological sort for chain alloy ordering.
/// Returns spark IDs in execution order, or error if a cycle exists.
pub fn topological_order(edges: &[(String, String)]) -> Result<Vec<String>, SparksError> {
    let mut graph = DiGraphMap::<&str, ()>::new();

    for (from, to) in edges {
        graph.add_edge(from.as_str(), to.as_str(), ());
    }

    match petgraph::algo::toposort(&graph, None) {
        Ok(order) => Ok(order.into_iter().map(|s| s.to_string()).collect()),
        Err(_) => Err(SparksError::CycleDetected {
            from: "chain".to_string(),
            to: "cycle in alloy members".to_string(),
        }),
    }
}
