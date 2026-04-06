// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! CRUD for Alloys (coordination templates) and their members.

use chrono::Utc;
use sqlx::SqlitePool;

use super::error::SparksError;
use super::id::generate_id;
use super::types::*;

pub struct AlloyMemberInput {
    pub spark_id: String,
    pub bond_type: AlloyBondType,
    pub position: i32,
}

/// Create an alloy with its member sparks.
/// The header INSERT and member INSERTs are wrapped in a transaction for atomicity.
pub async fn create(
    pool: &SqlitePool,
    new: NewAlloy,
    members: Vec<AlloyMemberInput>,
) -> Result<Alloy, SparksError> {
    let id = generate_id("al");
    let now = Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO alloys (id, name, alloy_type, parent_spark_id, workshop_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&new.name)
    .bind(new.alloy_type.as_str())
    .bind(&new.parent_spark_id)
    .bind(&new.workshop_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for member in members {
        sqlx::query(
            "INSERT INTO alloy_members (alloy_id, spark_id, bond_type, position) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&member.spark_id)
        .bind(member.bond_type.as_str())
        .bind(member.position)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    get(pool, &id).await
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Alloy, SparksError> {
    sqlx::query_as::<_, Alloy>("SELECT * FROM alloys WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SparksError::NotFound(format!("alloy {id}")))
}

pub async fn get_members(
    pool: &SqlitePool,
    alloy_id: &str,
) -> Result<Vec<AlloyMember>, SparksError> {
    Ok(sqlx::query_as::<_, AlloyMember>(
        "SELECT * FROM alloy_members WHERE alloy_id = ? ORDER BY position ASC",
    )
    .bind(alloy_id)
    .fetch_all(pool)
    .await?)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), SparksError> {
    let result = sqlx::query("DELETE FROM alloys WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(SparksError::NotFound(format!("alloy {id}")));
    }
    Ok(())
}

pub async fn list_for_workshop(
    pool: &SqlitePool,
    workshop_id: &str,
) -> Result<Vec<Alloy>, SparksError> {
    Ok(sqlx::query_as::<_, Alloy>(
        "SELECT * FROM alloys WHERE workshop_id = ? ORDER BY created_at DESC",
    )
    .bind(workshop_id)
    .fetch_all(pool)
    .await?)
}
