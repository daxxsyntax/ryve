// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Thin wrapper around the bundled tmux binary.
//!
//! All tmux invocations in Ryve go through this module so the rest of the
//! codebase never shells out to `tmux` directly. Every command uses a
//! Ryve-private socket (`-S <ryve-state-dir>/tmux.sock`) so they never
//! touch the user's default tmux server.
//!
//! Spark ryve-4bae4ff6 (wrapper module), ryve-8ba40d83 (attach UI).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the Ryve-private tmux socket for a workshop.
pub fn tmux_socket_path(workshop_dir: &Path) -> PathBuf {
    workshop_dir.join(".ryve").join("tmux.sock")
}

/// Canonical tmux session name for a Hand or Head.
///
/// Session names follow the pattern `hand-<session_id>` or
/// `head-<session_id>` so they are predictable and collision-free.
pub fn session_name_for(session_label: &str, session_id: &str) -> String {
    format!("{session_label}-{session_id}")
}

/// Resolve the tmux binary. For now we look for `tmux` on `$PATH`;
/// once the bundled-binary story is settled this will point at the
/// artifact under `.ryve/bin/tmux`.
fn tmux_bin() -> PathBuf {
    PathBuf::from("tmux")
}

/// Check whether a tmux session with the given name exists on the
/// Ryve-private socket. Returns `false` if the socket file doesn't
/// exist or the `has-session` command fails for any reason (binary
/// missing, etc.) — callers use this as a gating check for UI
/// affordances and must degrade gracefully.
pub fn has_session(workshop_dir: &Path, session_name: &str) -> bool {
    let socket = tmux_socket_path(workshop_dir);
    if !socket.exists() {
        return false;
    }
    Command::new(tmux_bin())
        .args([
            "-S",
            &socket.to_string_lossy(),
            "has-session",
            "-t",
            session_name,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build a `Command` that attaches to an existing tmux session on the
/// Ryve-private socket. The caller is expected to use this as the
/// program+args for an `iced_term` PTY — the terminal widget runs the
/// command and the user gets a live, interactive tmux client.
///
/// The returned command is **not** spawned here; it's the caller's job
/// to extract `(program, args)` and feed them to `iced_term::Settings`.
pub fn attach_command(workshop_dir: &Path, session_name: &str) -> (String, Vec<String>) {
    let socket = tmux_socket_path(workshop_dir);
    let bin = tmux_bin().to_string_lossy().into_owned();
    let args = vec![
        "-S".to_string(),
        socket.to_string_lossy().into_owned(),
        "attach".to_string(),
        "-t".to_string(),
        session_name.to_string(),
    ];
    (bin, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_is_under_ryve_dir() {
        let p = tmux_socket_path(Path::new("/tmp/workshop"));
        assert_eq!(p, PathBuf::from("/tmp/workshop/.ryve/tmux.sock"));
    }

    #[test]
    fn session_name_format() {
        assert_eq!(session_name_for("hand", "abc-123"), "hand-abc-123");
        assert_eq!(session_name_for("head", "xyz-456"), "head-xyz-456");
    }

    #[test]
    fn attach_command_uses_private_socket() {
        let (bin, args) = attach_command(Path::new("/ws"), "hand-abc");
        assert_eq!(bin, "tmux");
        assert!(args.contains(&"-S".to_string()));
        assert!(args.contains(&"/ws/.ryve/tmux.sock".to_string()));
        assert!(args.contains(&"attach".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"hand-abc".to_string()));
    }

    #[test]
    fn has_session_returns_false_when_socket_missing() {
        // No tmux socket at this path → should return false, not error.
        assert!(!has_session(Path::new("/nonexistent/workshop"), "hand-xyz"));
    }
}
