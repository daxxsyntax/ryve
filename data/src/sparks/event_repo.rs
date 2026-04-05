// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Append-only audit trail for spark changes.

use chrono::Utc;
use sqlx::SqlitePool;

use super::error::SparksError;
use super::types::*;

pub async fn record(pool: &SqlitePool, new: NewEvent) -> Result<(), SparksError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO events (spark_id, actor, field_name, old_value, new_value, reason, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new.spark_id)
    .bind(&new.actor)
    .bind(&new.field_name)
    .bind(&new.old_value)
    .bind(&new.new_value)
    .bind(&new.reason)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_for_spark(pool: &SqlitePool, spark_id: &str) -> Result<Vec<Event>, SparksError> {
    Ok(
        sqlx::query_as::<_, Event>(
            "SELECT * FROM events WHERE spark_id = ? ORDER BY timestamp ASC",
        )
        .bind(spark_id)
        .fetch_all(pool)
        .await?,
    )
}
