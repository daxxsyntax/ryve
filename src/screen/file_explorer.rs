// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! File explorer panel — displays project tree with git/worktree awareness.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    SelectFile(PathBuf),
    ToggleDirectory(PathBuf),
    SwitchWorktree(String),
}
