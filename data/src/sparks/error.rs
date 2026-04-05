// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

#[derive(Debug, thiserror::Error)]
pub enum SparksError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("cycle detected: bond from {from} to {to} would create a cycle")]
    CycleDetected { from: String, to: String },

    #[error("GitHub sync error: {0}")]
    GitHubSync(String),

    #[error("invalid status: {0}")]
    InvalidStatus(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
