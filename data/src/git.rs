// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Git and worktree operations via the `git` CLI.

use std::path::PathBuf;

use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct Repository {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
}

impl Repository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// List all worktrees for this repository.
    pub async fn list_worktrees(&self) -> Result<Vec<Worktree>, GitError> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.path)
            .output()
            .await
            .map_err(GitError::Io)?;

        if !output.status.success() {
            return Err(GitError::Command(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_worktree_list(&stdout))
    }
}

fn parse_worktree_list(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;

    for line in output.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = Some(b.to_string());
        } else if line.is_empty() {
            if let (Some(p), Some(b)) = (path.take(), branch.take()) {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                worktrees.push(Worktree {
                    name,
                    path: p,
                    branch: b,
                });
            }
            path = None;
            branch = None;
        }
    }

    worktrees
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("git command failed: {0}")]
    Command(String),
}
