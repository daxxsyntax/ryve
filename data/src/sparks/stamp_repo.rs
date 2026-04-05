// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! CRUD for Stamps (labels on sparks).

use sqlx::SqlitePool;

use super::error::SparksError;
use super::types::Stamp;

pub async fn add(pool: &SqlitePool, spark_id: &str, name: &str) -> Result<(), SparksError> {
    sqlx::query("INSERT OR IGNORE INTO stamps (spark_id, name) VALUES (?, ?)")
        .bind(spark_id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove(pool: &SqlitePool, spark_id: &str, name: &str) -> Result<(), SparksError> {
    sqlx::query("DELETE FROM stamps WHERE spark_id = ? AND name = ?")
        .bind(spark_id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Replace all stamps for a spark with the given list.
pub async fn set(pool: &SqlitePool, spark_id: &str, names: &[&str]) -> Result<(), SparksError> {
    sqlx::query("DELETE FROM stamps WHERE spark_id = ?")
        .bind(spark_id)
        .execute(pool)
        .await?;

    for name in names {
        sqlx::query("INSERT INTO stamps (spark_id, name) VALUES (?, ?)")
            .bind(spark_id)
            .bind(name)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn list_for_spark(pool: &SqlitePool, spark_id: &str) -> Result<Vec<Stamp>, SparksError> {
    Ok(
        sqlx::query_as::<_, Stamp>("SELECT * FROM stamps WHERE spark_id = ?")
            .bind(spark_id)
            .fetch_all(pool)
            .await?,
    )
}
