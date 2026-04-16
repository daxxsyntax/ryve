// SPDX-License-Identifier: AGPL-3.0-or-later

//! CRUD for Engravings (persistent shared knowledge).

use chrono::Utc;
use sqlx::SqlitePool;

use super::error::SparksError;
use super::types::*;

/// Insert or update an engraving. If the key already exists for the workshop,
/// the value and updated_at are overwritten.
pub async fn upsert(pool: &SqlitePool, new: NewEngraving) -> Result<Engraving, SparksError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO engravings (key, workshop_id, value, author, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(key, workshop_id) DO UPDATE SET value=excluded.value, author=excluded.author, updated_at=excluded.updated_at",
    )
    .bind(&new.key)
    .bind(&new.workshop_id)
    .bind(&new.value)
    .bind(&new.author)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get(pool, &new.key, &new.workshop_id).await
}

pub async fn get(
    pool: &SqlitePool,
    key: &str,
    workshop_id: &str,
) -> Result<Engraving, SparksError> {
    sqlx::query_as::<_, Engraving>("SELECT * FROM engravings WHERE key = ? AND workshop_id = ?")
        .bind(key)
        .bind(workshop_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SparksError::NotFound(format!("engraving {key}")))
}

pub async fn delete(pool: &SqlitePool, key: &str, workshop_id: &str) -> Result<(), SparksError> {
    let result = sqlx::query("DELETE FROM engravings WHERE key = ? AND workshop_id = ?")
        .bind(key)
        .bind(workshop_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(SparksError::NotFound(format!("engraving {key}")));
    }
    Ok(())
}

pub async fn list_for_workshop(
    pool: &SqlitePool,
    workshop_id: &str,
) -> Result<Vec<Engraving>, SparksError> {
    Ok(sqlx::query_as::<_, Engraving>(
        "SELECT * FROM engravings WHERE workshop_id = ? ORDER BY key ASC",
    )
    .bind(workshop_id)
    .fetch_all(pool)
    .await?)
}
