// SPDX-License-Identifier: AGPL-3.0-or-later

//! CRUD for Comments on sparks.

use chrono::Utc;
use sqlx::SqlitePool;

use super::error::SparksError;
use super::id::generate_id;
use super::types::*;

pub async fn create(pool: &SqlitePool, new: NewComment) -> Result<Comment, SparksError> {
    let id = generate_id("cm");
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO comments (id, spark_id, author, body, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&new.spark_id)
    .bind(&new.author)
    .bind(&new.body)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(Comment {
        id,
        spark_id: new.spark_id,
        author: new.author,
        body: new.body,
        created_at: now,
    })
}

pub async fn list_for_spark(
    pool: &SqlitePool,
    spark_id: &str,
) -> Result<Vec<Comment>, SparksError> {
    Ok(sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE spark_id = ? ORDER BY created_at ASC",
    )
    .bind(spark_id)
    .fetch_all(pool)
    .await?)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), SparksError> {
    let result = sqlx::query("DELETE FROM comments WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(SparksError::NotFound(format!("comment {id}")));
    }
    Ok(())
}
