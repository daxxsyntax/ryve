// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Tmux wrapper with private-socket discipline.
//!
//! Every Ryve workshop keeps its agent sessions on a **private tmux socket**
//! (`<workshop>/.ryve/tmux.sock`) so they don't collide with the user's
//! personal tmux sessions. This module provides:
//!
//! - Session naming conventions (`hand-<short>`, `head-<short>`)
//! - Listing live sessions on the private socket
//! - Launching a command inside a new tmux session
//! - Checking whether a named session still exists
//! - **Reconciliation**: on startup, match existing tmux sessions against
//!   `agent_sessions` DB rows so the UI resumes correctly after a Ryve restart.
//!
//! Spark [sp-0285181c].

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use data::sparks::types::PersistedAgentSession;
use sqlx::SqlitePool;

// ── Socket path ──────────────────────────────────────

/// Return the path to the private tmux socket for a workshop.
///
/// Unix domain sockets have a ~104-byte path limit on macOS (and 108 on
/// Linux). Workshop directories can live deep inside `/var/folders/...`
/// or similar, easily exceeding this. We therefore place the socket under
/// `/tmp/ryve-tmux-<hash>` where `<hash>` is a short hex digest of the
/// canonical workshop path. This keeps the socket path short while still
/// being deterministic (the same workshop always gets the same socket).
pub fn socket_path(workshop_dir: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let canonical = workshop_dir
        .canonicalize()
        .unwrap_or_else(|_| workshop_dir.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    let hash = hasher.finish();

    PathBuf::from(format!("/tmp/ryve-tmux-{hash:016x}"))
}

// ── Session naming ───────────────────────────────────

/// The canonical tmux session name for a given agent session.
///
/// Format: `<label>-<short_id>` where `short_id` is the first 8 chars of
/// the UUID. This matches the branch naming used by `create_hand_worktree`
/// (`hand/<short_id>`), keeping the mapping trivially reversible.
pub fn session_name(session_label: Option<&str>, session_id: &str) -> String {
    let label = session_label.unwrap_or("hand");
    let short = &session_id[..8.min(session_id.len())];
    format!("{label}-{short}")
}

/// Returns true if `name` looks like a Ryve-managed tmux session
/// (`hand-*`, `head-*`, or `merger-*`).
fn is_ryve_session(name: &str) -> bool {
    name.starts_with("hand-") || name.starts_with("head-") || name.starts_with("merger-")
}

// ── List sessions ────────────────────────────────────

/// A live tmux session discovered on the private socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
}

/// List all tmux sessions on the workshop's private socket.
///
/// Returns an empty vec (not an error) when:
/// - tmux is not installed
/// - the socket file doesn't exist
/// - no sessions are running
pub async fn list_sessions(workshop_dir: &Path) -> Vec<TmuxSession> {
    let sock = socket_path(workshop_dir);
    if !sock.exists() {
        return Vec::new();
    }
    list_sessions_on_socket(&sock).await
}

/// List sessions on an arbitrary socket path. Factored out for testability.
async fn list_sessions_on_socket(sock: &Path) -> Vec<TmuxSession> {
    let output = tokio::process::Command::new("tmux")
        .args([
            "-S",
            &sock.to_string_lossy(),
            "list-sessions",
            "-F",
            "#{session_name}",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| TmuxSession {
                    name: l.trim().to_string(),
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ── Shell escaping ───────────────────────────────────

/// Escape a string for safe use in a shell script (POSIX sh). Wraps the
/// value in single quotes, with any internal single quotes escaped as
/// `'\''` (end quote, escaped literal quote, start quote).
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Launch inside tmux ───────────────────────────────

/// Error from [`launch_in_tmux`].
#[derive(Debug, thiserror::Error)]
pub enum TmuxLaunchError {
    #[error("tmux launch failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("tmux new-session exited with status {0}")]
    NonZero(std::process::ExitStatus),
}

/// Launch a command inside a new detached tmux session on the workshop's
/// private socket.
///
/// The session is named according to [`session_name`]. stdout/stderr of
/// the inner command are captured inside the tmux pane (scrollback), and
/// additionally piped to `log_path` via `tee` so the existing log-tail
/// spy view keeps working.
///
/// Returns the tmux session name (not a PID — the tmux server owns the
/// process).
pub async fn launch_in_tmux(
    workshop_dir: &Path,
    tmux_session_name: &str,
    command: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    log_path: &Path,
) -> Result<String, TmuxLaunchError> {
    let sock = socket_path(workshop_dir);

    // Ensure the socket's parent directory exists.
    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Write a wrapper script that sets up env, cd's into the worktree,
    // and exec's the agent. This avoids shell-quoting issues from passing
    // a big `sh -c '...'` string to tmux — the prompt text in args can
    // contain arbitrary characters (quotes, newlines, backticks, etc.).
    let wrapper_dir = workshop_dir.join(".ryve").join("tmux-wrappers");
    tokio::fs::create_dir_all(&wrapper_dir).await?;
    let wrapper_path = wrapper_dir.join(format!("{tmux_session_name}.sh"));

    let mut script = String::from("#!/bin/sh\nset -e\n");
    for (k, v) in env {
        // Use printf to avoid interpretation of backslashes.
        script.push_str(&format!(
            "export {}=\"$(printf '%s' \"${{0+{}}}\")\"\n",
            k,
            shell_escape(v)
        ));
    }
    // Actually, let's use a heredoc-free approach: write env to the script
    // with proper escaping.
    script.clear();
    script.push_str("#!/bin/sh\nset -e\n");
    for (k, v) in env {
        let escaped = shell_escape(v);
        script.push_str(&format!("export {k}={escaped}\n"));
    }
    let cwd_escaped = shell_escape(&cwd.to_string_lossy());
    script.push_str(&format!("cd {cwd_escaped}\n"));

    // Pipe through tee so the log file keeps working for the spy view.
    let log_escaped = shell_escape(&log_path.to_string_lossy());
    let cmd_escaped = shell_escape(command);
    script.push_str(&format!("exec {cmd_escaped}"));
    for arg in args {
        script.push_str(&format!(" {}", shell_escape(arg)));
    }
    script.push_str(&format!(" 2>&1 | tee -a {log_escaped}\n"));

    tokio::fs::write(&wrapper_path, &script).await?;

    // Make the wrapper executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&wrapper_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&wrapper_path, perms).await?;
    }

    let output = tokio::process::Command::new("tmux")
        .args([
            "-S",
            &sock.to_string_lossy(),
            "new-session",
            "-d",
            "-s",
            tmux_session_name,
            wrapper_path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!(
            "tmux new-session failed (status={}): {}",
            output.status,
            stderr,
        );
        return Err(TmuxLaunchError::NonZero(output.status));
    }

    Ok(tmux_session_name.to_string())
}

// ── Reconciliation ───────────────────────────────────

/// Outcome of one reconciliation run, for logging and testing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReconcileResult {
    /// Session IDs that were confirmed live (tmux session exists).
    pub confirmed_live: Vec<String>,
    /// Session IDs that were marked stopped (no tmux session found).
    pub marked_stopped: Vec<String>,
    /// Tmux session names that have no matching DB row (orphans).
    pub orphaned_tmux: Vec<String>,
}

/// Reconcile existing tmux sessions on the private socket with
/// `agent_sessions` DB rows.
///
/// Called on app boot (after the DB is opened) to restore liveness state
/// that was lost when Ryve exited while tmux sessions kept running.
///
/// **Behaviour:**
/// - Active DB rows whose tmux session still exists → left as-is (live).
/// - Active DB rows whose tmux session is gone → marked `ended`.
/// - Tmux sessions with no matching DB row → logged, left alone.
///
/// Idempotent: safe to call repeatedly.
pub async fn reconcile_sessions(
    workshop_dir: &Path,
    pool: &SqlitePool,
    workshop_id: &str,
) -> ReconcileResult {
    let mut result = ReconcileResult::default();

    // 1. Discover what tmux sessions exist on the private socket.
    let live_tmux = list_sessions(workshop_dir).await;
    let live_names: HashSet<&str> = live_tmux.iter().map(|s| s.name.as_str()).collect();

    // 2. Load all active DB sessions.
    let db_sessions: Vec<PersistedAgentSession> =
        data::sparks::agent_session_repo::list_for_workshop(pool, workshop_id)
            .await
            .unwrap_or_default();

    let active_sessions: Vec<&PersistedAgentSession> = db_sessions
        .iter()
        .filter(|s| s.status == "active")
        .collect();

    // 3. Build a set of expected tmux names from active DB rows.
    let mut expected_names: HashSet<String> = HashSet::new();
    let mut name_to_session_id: std::collections::HashMap<String, &str> =
        std::collections::HashMap::new();

    for s in &active_sessions {
        let name = session_name(s.session_label.as_deref(), &s.id);
        expected_names.insert(name.clone());
        name_to_session_id.insert(name, &s.id);
    }

    // 4. For each active DB row, check if its tmux session exists.
    for s in &active_sessions {
        let name = session_name(s.session_label.as_deref(), &s.id);
        if live_names.contains(name.as_str()) {
            log::info!(
                "tmux reconcile: session '{}' (db={}) confirmed live",
                name,
                s.id
            );
            result.confirmed_live.push(s.id.clone());
        } else {
            log::info!(
                "tmux reconcile: session '{}' (db={}) has no tmux session — marking stopped",
                name,
                s.id
            );
            let _ = data::sparks::agent_session_repo::end_session(pool, &s.id).await;
            result.marked_stopped.push(s.id.clone());
        }
    }

    // 5. Report tmux sessions that have no matching DB row.
    for ts in &live_tmux {
        if is_ryve_session(&ts.name) && !expected_names.contains(&ts.name) {
            log::warn!(
                "tmux reconcile: tmux session '{}' has no matching agent_sessions row — leaving alone",
                ts.name
            );
            result.orphaned_tmux.push(ts.name.clone());
        }
    }

    result
}

/// Kill the tmux server on the workshop's private socket. Used during
/// test cleanup to terminate all sessions at once.
#[cfg(test)]
pub(crate) async fn kill_server(workshop_dir: &Path) {
    let sock = socket_path(workshop_dir);
    if !sock.exists() {
        return;
    }
    let _ = tokio::process::Command::new("tmux")
        .args(["-S", &sock.to_string_lossy(), "kill-server"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

// ── Tests ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_hand() {
        assert_eq!(
            session_name(Some("hand"), "abcd1234-5678-9abc-def0-123456789abc"),
            "hand-abcd1234"
        );
    }

    #[test]
    fn session_name_head() {
        assert_eq!(
            session_name(Some("head"), "12345678-abcd-ef01-2345-6789abcdef01"),
            "head-12345678"
        );
    }

    #[test]
    fn session_name_merger() {
        assert_eq!(
            session_name(Some("merger"), "deadbeef-1234-5678-9abc-def012345678"),
            "merger-deadbeef"
        );
    }

    #[test]
    fn session_name_none_label_defaults_to_hand() {
        assert_eq!(
            session_name(None, "abcd1234-0000-0000-0000-000000000000"),
            "hand-abcd1234"
        );
    }

    #[test]
    fn is_ryve_session_detection() {
        assert!(is_ryve_session("hand-abcd1234"));
        assert!(is_ryve_session("head-12345678"));
        assert!(is_ryve_session("merger-deadbeef"));
        assert!(!is_ryve_session("my-session"));
        assert!(!is_ryve_session("hands-off"));
    }

    /// Reconciliation with an empty DB and no tmux sessions → no-op.
    #[tokio::test]
    async fn reconcile_empty_is_noop() {
        let dir = std::env::temp_dir().join(format!("ryve-tmux-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let pool = data::db::open_sparks_db(&dir).await.unwrap();
        let result = reconcile_sessions(&dir, &pool, "test-ws").await;

        assert_eq!(result, ReconcileResult::default());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DB row with status=active but no tmux session → marked stopped.
    #[tokio::test]
    async fn reconcile_marks_orphaned_db_rows_stopped() {
        let dir = std::env::temp_dir().join(format!("ryve-tmux-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let pool = data::db::open_sparks_db(&dir).await.unwrap();
        let ws_id = "test-ws";

        // Insert an active session that has no matching tmux session.
        let session = data::sparks::types::NewAgentSession {
            id: "abcd1234-0000-0000-0000-000000000000".to_string(),
            workshop_id: ws_id.to_string(),
            agent_name: "Claude Code".to_string(),
            agent_command: "claude".to_string(),
            agent_args: Vec::new(),
            session_label: Some("hand".to_string()),
            child_pid: None,
            resume_id: None,
            log_path: None,
            parent_session_id: None,
        };
        data::sparks::agent_session_repo::create(&pool, &session)
            .await
            .unwrap();

        let result = reconcile_sessions(&dir, &pool, ws_id).await;

        assert!(result.confirmed_live.is_empty());
        assert_eq!(result.marked_stopped, vec![session.id.clone()]);
        assert!(result.orphaned_tmux.is_empty());

        // Verify the DB row was actually ended.
        let rows = data::sparks::agent_session_repo::list_for_workshop(&pool, ws_id)
            .await
            .unwrap();
        let row = rows.iter().find(|r| r.id == session.id).unwrap();
        assert_eq!(row.status, "ended");
        assert!(row.ended_at.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reconciliation is idempotent: running it twice on the same state
    /// produces the same result (no double-ending, no errors).
    #[tokio::test]
    async fn reconcile_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("ryve-tmux-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let pool = data::db::open_sparks_db(&dir).await.unwrap();
        let ws_id = "test-ws";

        let session = data::sparks::types::NewAgentSession {
            id: "deadbeef-0000-0000-0000-000000000000".to_string(),
            workshop_id: ws_id.to_string(),
            agent_name: "Claude Code".to_string(),
            agent_command: "claude".to_string(),
            agent_args: Vec::new(),
            session_label: Some("hand".to_string()),
            child_pid: None,
            resume_id: None,
            log_path: None,
            parent_session_id: None,
        };
        data::sparks::agent_session_repo::create(&pool, &session)
            .await
            .unwrap();

        // First run: marks it stopped.
        let r1 = reconcile_sessions(&dir, &pool, ws_id).await;
        assert_eq!(r1.marked_stopped.len(), 1);

        // Second run: the row is already ended, so it's no longer "active"
        // and reconciliation has nothing to do.
        let r2 = reconcile_sessions(&dir, &pool, ws_id).await;
        assert!(r2.confirmed_live.is_empty());
        assert!(r2.marked_stopped.is_empty());
        assert!(r2.orphaned_tmux.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
