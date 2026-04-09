// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Build script for the `ryve` binary crate.
//!
//! Responsibilities:
//! - Read the pinned tmux version from `vendor/tmux/VERSION` and expose it as
//!   `RYVE_TMUX_VERSION` so `src/bundled_tmux.rs` can embed it at compile time.
//! - Set `RYVE_TMUX_DEV_PATH` to the development-layout path
//!   (`<repo>/vendor/tmux/bin/tmux`) for the dev-build fallback.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Read the pinned tmux version.
    let version_file = manifest_dir.join("vendor/tmux/VERSION");
    let version = std::fs::read_to_string(&version_file)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", version_file.display()));
    let version = version.trim();

    println!("cargo:rustc-env=RYVE_TMUX_VERSION={version}");

    // Development-layout path for the bundled tmux binary.
    let dev_path = manifest_dir.join("vendor/tmux/bin/tmux");
    println!("cargo:rustc-env=RYVE_TMUX_DEV_PATH={}", dev_path.display());

    // Rebuild if the version file changes.
    println!("cargo:rerun-if-changed=vendor/tmux/VERSION");
}
