// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Git and worktree operations via the `git` CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Git status of a single file relative to the repo root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Ignored,
    Conflicted,
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

    /// Get the current branch name (or HEAD if detached).
    pub async fn current_branch(&self) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.path)
            .output()
            .await
            .map_err(GitError::Io)?;

        if !output.status.success() {
            return Err(GitError::Command(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get file-level git status for the working tree.
    /// Returns a map from repo-relative file path to its status.
    pub async fn file_statuses(&self) -> Result<HashMap<PathBuf, FileStatus>, GitError> {
        let output = Command::new("git")
            .args(["status", "--porcelain=v1", "-uall"])
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
        Ok(parse_status_porcelain(&stdout))
    }

    /// Check if a path is inside a git repository.
    pub async fn is_repo(path: &Path) -> bool {
        Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(path)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Parse `git status --porcelain=v1` output into a file status map.
fn parse_status_porcelain(output: &str) -> HashMap<PathBuf, FileStatus> {
    let mut statuses = HashMap::new();

    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }

        let index = line.as_bytes()[0];
        let worktree = line.as_bytes()[1];
        let file_path = &line[3..];

        // For renames, take the destination path (after " -> ")
        let path = if let Some(pos) = file_path.find(" -> ") {
            PathBuf::from(&file_path[pos + 4..])
        } else {
            PathBuf::from(file_path)
        };

        let status = match (index, worktree) {
            (b'?', b'?') => FileStatus::Untracked,
            (b'!', b'!') => FileStatus::Ignored,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => FileStatus::Conflicted,
            (b'A', _) => FileStatus::Added,
            (_, b'D') | (b'D', _) => FileStatus::Deleted,
            (b'R', _) => FileStatus::Renamed,
            (b'C', _) => FileStatus::Copied,
            _ => FileStatus::Modified,
        };

        statuses.insert(path, status);
    }

    statuses
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
