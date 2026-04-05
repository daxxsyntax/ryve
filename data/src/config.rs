// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the SQLite database file
    pub database_path: PathBuf,
    /// Root directory for projects
    pub workspace_dir: PathBuf,
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .expect("no config directory found")
            .join("forge")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_path: Self::config_dir().join("forge.db"),
            workspace_dir: dirs::home_dir()
                .expect("no home directory found")
                .join("dev"),
        }
    }
}
