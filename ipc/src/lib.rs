// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Inter-process communication for Forge.
//!
//! Handles single-instance enforcement and message passing
//! between Forge windows/processes.

use std::path::PathBuf;

/// Returns the path for the IPC socket.
pub fn socket_path() -> PathBuf {
    let dir = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(std::env::temp_dir);
    dir.join("forge.sock")
}
