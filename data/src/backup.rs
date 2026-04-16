// SPDX-License-Identifier: AGPL-3.0-or-later

//! Workgraph backup and restore.
//!
//! Ryve periodically snapshots each workshop's `sparks.db` into
//! `.ryve/backups/` so that a corrupted or accidentally-wiped database can
//! be recovered. Snapshots are taken via SQLite's `VACUUM INTO`, which
//! produces a fully consistent, self-contained copy of the database on a
//! live connection (WAL frames are applied before the copy is written).
//!
//! ## Layout
//!
//! ```text
//! .ryve/
//! └── backups/
//!     ├── sparks-20260408T130500Z.db
//!     ├── sparks-20260408T140500Z.db
//!     └── ...
//! ```
//!
//! Each snapshot is a complete SQLite database file. Restoring is simply
//! "copy the snapshot over `sparks.db`" — the helper in this module does
//! that safely by first moving the current (possibly corrupt) database
//! aside so the user can inspect it later.
//!
//! ## Retention
//!
//! [`apply_retention`] uses a daily/weekly taper: it keeps the newest N
//! snapshots, the newest-per-day for the last D days, and the
//! newest-per-week for the last W weeks. The UI calls this after every
//! successful snapshot.
//!
//! ## Recovery
//!
//! See `docs/RECOVERY.md` and the `ryve restore` CLI subcommand.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, IsoWeek, Utc};
use sqlx::SqlitePool;

use crate::ryve_dir::RyveDir;

/// Default periodic snapshot interval for the UI. Chosen to be long
/// enough that a busy workshop isn't spamming the disk on every tick but
/// short enough that at most a few minutes of work is ever lost.
pub const DEFAULT_BACKUP_INTERVAL_SECS: u64 = 600; // 10 minutes

/// Number of most-recent snapshots to always keep regardless of age.
/// 48 × 10 min ≈ 8 hours of continuous coverage.
pub const KEEP_RECENT: usize = 48;

/// Number of days for which the newest-per-day snapshot is preserved
/// beyond the KEEP_RECENT window.
pub const KEEP_DAILY_DAYS: u32 = 30;

/// Number of weeks for which the newest-per-week snapshot is preserved
/// beyond the KEEP_RECENT and daily windows.
pub const KEEP_WEEKLY_WEEKS: u32 = 12;

/// Retention policy controlling which snapshots survive pruning.
#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub keep_recent: usize,
    pub keep_daily_days: u32,
    pub keep_weekly_weeks: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_recent: KEEP_RECENT,
            keep_daily_days: KEEP_DAILY_DAYS,
            keep_weekly_weeks: KEEP_WEEKLY_WEEKS,
        }
    }
}

/// Prefix used for snapshot file names: `sparks-<ISO8601>.db`. Anything
/// in `.ryve/backups/` not matching this prefix is ignored by listing and
/// retention so users can drop their own archival copies in the dir
/// without losing them to pruning.
pub const SNAPSHOT_PREFIX: &str = "sparks-";
pub const SNAPSHOT_EXT: &str = "db";

/// Errors produced by backup/restore operations. Kept intentionally
/// simple — callers typically surface these as a toast or log line.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("snapshot not found: {0}")]
    NotFound(PathBuf),
    #[error("{0}")]
    Other(String),
}

/// Metadata about a single snapshot file on disk.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Full path to the snapshot file.
    pub path: PathBuf,
    /// Timestamp parsed out of the filename. `None` if the name uses
    /// the prefix but we couldn't parse a timestamp — still listed so
    /// the user can see the file exists.
    pub taken_at: Option<DateTime<Utc>>,
    /// File size in bytes.
    pub size: u64,
}

impl Snapshot {
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Format a UTC timestamp into the filename form used by snapshots:
/// `20260408T130500Z`. Deterministic and lexicographically sortable, so
/// sorting by filename yields chronological order.
pub fn format_stamp(ts: DateTime<Utc>) -> String {
    ts.format("%Y%m%dT%H%M%SZ").to_string()
}

/// Parse a snapshot filename like `sparks-20260408T130500Z.db` into its
/// timestamp component. Returns `None` for names that don't match.
pub fn parse_stamp(file_name: &str) -> Option<DateTime<Utc>> {
    let rest = file_name.strip_prefix(SNAPSHOT_PREFIX)?;
    let stamp = rest.strip_suffix(&format!(".{SNAPSHOT_EXT}"))?;
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

/// Build the path where a snapshot taken at `ts` should live.
pub fn snapshot_path(ryve_dir: &RyveDir, ts: DateTime<Utc>) -> PathBuf {
    ryve_dir.backups_dir().join(format!(
        "{SNAPSHOT_PREFIX}{}.{SNAPSHOT_EXT}",
        format_stamp(ts)
    ))
}

/// Take a snapshot of the live database backing `pool` and write it to
/// `dest`. Uses `VACUUM INTO`, which is atomic with respect to other
/// writers sharing the pool: the snapshot reflects a consistent point
/// in time even under concurrent writes.
///
/// The destination path must not already exist (SQLite refuses to
/// overwrite). Callers using [`take_snapshot`] get unique timestamped
/// paths automatically.
pub async fn snapshot_to(pool: &SqlitePool, dest: &Path) -> Result<(), BackupError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if dest.exists() {
        return Err(BackupError::Other(format!(
            "refusing to overwrite existing snapshot at {}",
            dest.display()
        )));
    }
    // SQLite's VACUUM INTO does not accept bound parameters; we have to
    // interpolate. Escape embedded single quotes so paths with `'` in
    // them don't break the statement.
    let path_str = dest.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{path_str}'");
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

/// Take a timestamped snapshot into `.ryve/backups/` and return the
/// resulting file path. Uses the current UTC time as the stamp. If
/// the base timestamped filename already exists (e.g. the periodic
/// tick and a graceful-close snapshot land in the same second), retries
/// with a disambiguating numeric suffix so multiple snapshots in the
/// same second do not fail.
pub async fn take_snapshot(pool: &SqlitePool, ryve_dir: &RyveDir) -> Result<PathBuf, BackupError> {
    let ts = Utc::now();
    let base_dest = snapshot_path(ryve_dir, ts);

    for suffix in 0u32..u32::MAX {
        let dest = if suffix == 0 {
            base_dest.clone()
        } else {
            let file_stem = base_dest
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    BackupError::Other(format!(
                        "invalid snapshot file name: {}",
                        base_dest.display()
                    ))
                })?;
            let ext = base_dest
                .extension()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    BackupError::Other(format!(
                        "invalid snapshot file extension: {}",
                        base_dest.display()
                    ))
                })?;
            let file_name = format!("{file_stem}-{suffix}.{ext}");
            base_dest.with_file_name(file_name)
        };

        if dest.exists() {
            continue;
        }

        match snapshot_to(pool, &dest).await {
            Ok(()) => return Ok(dest),
            Err(BackupError::Other(msg))
                if msg.starts_with("refusing to overwrite existing snapshot at ") =>
            {
                continue;
            }
            Err(err) => return Err(err),
        }
    }

    Err(BackupError::Other(format!(
        "exhausted snapshot suffixes for {}",
        base_dest.display()
    )))
}

/// List all snapshots in `.ryve/backups/`, sorted oldest → newest by
/// parsed timestamp (falling back to filename order when the stamp can't
/// be parsed). Non-matching files are ignored.
pub async fn list_snapshots(ryve_dir: &RyveDir) -> Result<Vec<Snapshot>, BackupError> {
    let dir = ryve_dir.backups_dir();
    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !file_name.starts_with(SNAPSHOT_PREFIX)
            || !file_name.ends_with(&format!(".{SNAPSHOT_EXT}"))
        {
            continue;
        }
        let meta = entry.metadata().await?;
        if !meta.is_file() {
            continue;
        }
        out.push(Snapshot {
            taken_at: parse_stamp(&file_name),
            size: meta.len(),
            path,
        });
    }
    out.sort_by(|a, b| match (a.taken_at, b.taken_at) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => a.path.cmp(&b.path),
    });
    Ok(out)
}

/// Compute the set of snapshot indices that the retention policy preserves.
///
/// The returned indices refer to the *input* `snapshots` slice, which
/// [`list_snapshots`] produces in oldest-first order. Internally the function
/// walks newest-first so retention windows are easy to express, but the
/// indices stored in the set always reference the original oldest-first
/// order — callers can simply `enumerate()` over `snapshots` and check
/// membership. Exposed for testability.
pub fn retained_indices(
    snapshots: &[Snapshot],
    policy: &RetentionPolicy,
    now: DateTime<Utc>,
) -> HashSet<usize> {
    let mut keep_set = HashSet::new();
    if snapshots.is_empty() {
        return keep_set;
    }

    // snapshots arrive oldest-first from list_snapshots; work newest-first.
    let newest_first: Vec<_> = snapshots.iter().enumerate().rev().collect();

    // (a) Always keep the most recent snapshot.
    keep_set.insert(newest_first[0].0);

    // (b) Keep the newest KEEP_RECENT snapshots.
    for &(idx, _) in newest_first.iter().take(policy.keep_recent) {
        keep_set.insert(idx);
    }

    // (c) Newest-per-day for the last keep_daily_days days.
    let daily_cutoff = now - Duration::days(i64::from(policy.keep_daily_days));
    let mut seen_days: HashSet<chrono::NaiveDate> = HashSet::new();
    for &(idx, snap) in &newest_first {
        if let Some(ts) = snap.taken_at {
            if ts < daily_cutoff {
                continue;
            }
            let day = ts.date_naive();
            if seen_days.insert(day) {
                keep_set.insert(idx);
            }
        }
    }

    // (d) Newest-per-week for the last keep_weekly_weeks weeks.
    let weekly_cutoff = now - Duration::weeks(i64::from(policy.keep_weekly_weeks));
    let mut seen_weeks: HashSet<IsoWeek> = HashSet::new();
    for &(idx, snap) in &newest_first {
        if let Some(ts) = snap.taken_at {
            if ts < weekly_cutoff {
                continue;
            }
            let week = ts.date_naive().iso_week();
            if seen_weeks.insert(week) {
                keep_set.insert(idx);
            }
        }
    }

    keep_set
}

/// Prune snapshots according to the taper retention policy: keep the
/// newest `keep_recent`, plus one-per-day for `keep_daily_days`, plus
/// one-per-week for `keep_weekly_weeks`. The most recent snapshot is
/// never deleted. Returns the paths that were removed.
pub async fn apply_retention(
    ryve_dir: &RyveDir,
    policy: &RetentionPolicy,
) -> Result<Vec<PathBuf>, BackupError> {
    let snapshots = list_snapshots(ryve_dir).await?;
    if snapshots.is_empty() {
        return Ok(Vec::new());
    }

    let keep_set = retained_indices(&snapshots, policy, Utc::now());

    let mut deleted = Vec::new();
    for (idx, snap) in snapshots.into_iter().enumerate() {
        if keep_set.contains(&idx) {
            continue;
        }
        match tokio::fs::remove_file(&snap.path).await {
            Ok(()) => deleted.push(snap.path),
            Err(e) => {
                log::warn!("backup: failed to prune {}: {e}", snap.path.display());
            }
        }
    }
    Ok(deleted)
}

/// Convenience: take a snapshot, then prune according to the retention
/// policy. This is what the UI timer and shutdown hook call.
pub async fn snapshot_and_retain(
    pool: &SqlitePool,
    ryve_dir: &RyveDir,
    policy: &RetentionPolicy,
) -> Result<PathBuf, BackupError> {
    let path = take_snapshot(pool, ryve_dir).await?;
    let _ = apply_retention(ryve_dir, policy).await?;
    Ok(path)
}

/// Result of a successful [`restore_snapshot`] call.
#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    /// The snapshot that was copied into place.
    pub snapshot: PathBuf,
    /// Where the previous `sparks.db` was moved (if it existed). The
    /// user may want to delete or archive this.
    pub previous_db_backup: Option<PathBuf>,
    /// The live database path after restoration.
    pub restored_db: PathBuf,
}

/// Restore `sparks.db` from a snapshot file. Safe to call while no
/// Ryve process has the database open — the UI should be shut down
/// first, and the CLI ensures no pool is held open against the target
/// during the copy.
///
/// The current `sparks.db` (and its WAL/SHM sidecars) are moved aside
/// to `sparks.db.pre-restore-<stamp>.bak` before the snapshot is copied
/// into place. This gives the user a chance to recover their existing
/// state if they restored the wrong snapshot.
///
/// `snapshot` may be a bare filename (resolved against
/// `.ryve/backups/`) or an absolute/relative path to any SQLite file.
pub async fn restore_snapshot(
    ryve_dir: &RyveDir,
    snapshot: &Path,
) -> Result<RestoreOutcome, BackupError> {
    let snapshot_path = resolve_snapshot(ryve_dir, snapshot);
    if !snapshot_path.exists() {
        return Err(BackupError::NotFound(snapshot_path));
    }

    let live_db = ryve_dir.sparks_db_path();
    let stamp = format_stamp(Utc::now());

    // Move the existing database aside. We do this instead of deleting
    // so the user can undo a bad restore.
    let previous = if live_db.exists() {
        let bak = live_db.with_extension(format!("db.pre-restore-{stamp}.bak"));
        tokio::fs::rename(&live_db, &bak).await?;
        Some(bak)
    } else {
        None
    };

    // SQLite sidecars must be cleared even if the main DB file is
    // already missing — stale WAL/SHM/journal files can block opening
    // the restored database or apply unintended state against it. We
    // handle these independently of whether `sparks.db` existed.
    for ext in ["db-wal", "db-shm", "db-journal"] {
        let side = live_db.with_extension(ext);
        if side.exists() {
            let side_bak = live_db.with_extension(format!("{ext}.pre-restore-{stamp}.bak"));
            // Sidecar rename failures are non-fatal — log and continue
            // so restore can still proceed with the snapshot copy.
            if let Err(e) = tokio::fs::rename(&side, &side_bak).await {
                log::warn!("backup: failed to move sidecar {}: {e}", side.display());
            }
        }
    }

    // Ensure the parent dir exists, then restore via a temp sibling so
    // the final move into place is atomic. An interrupted copy leaves
    // only the temp file behind — never a torn `sparks.db`.
    if let Some(parent) = live_db.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temp_restore = live_db.with_extension(format!("db.restore-{stamp}.tmp"));
    tokio::fs::copy(&snapshot_path, &temp_restore).await?;
    tokio::fs::rename(&temp_restore, &live_db).await?;

    Ok(RestoreOutcome {
        snapshot: snapshot_path,
        previous_db_backup: previous,
        restored_db: live_db,
    })
}

/// Resolve a user-supplied snapshot identifier to a concrete path. If
/// the identifier is an absolute or existing relative path, it's used
/// as-is; otherwise it's looked up inside `.ryve/backups/`.
pub fn resolve_snapshot(ryve_dir: &RyveDir, snapshot: &Path) -> PathBuf {
    if snapshot.is_absolute() || snapshot.exists() {
        return snapshot.to_path_buf();
    }
    ryve_dir.backups_dir().join(snapshot)
}

const BACKUP_FLARE_PREFIX: &str = "Backup failed: ";
const BACKUP_FLARE_COALESCE_SECS: i64 = 600;

/// Emit a flare ember for a backup failure, coalescing repeated failures
/// within a 10-minute window. The first failure creates a new flare;
/// subsequent failures update the existing ember's content with a count.
pub async fn emit_backup_failure_flare(
    pool: &sqlx::SqlitePool,
    workshop_id: &str,
    error: &str,
) -> Result<(), crate::sparks::error::SparksError> {
    use crate::sparks::ember_repo;
    use crate::sparks::types::{EmberType, NewEmber};

    let existing = ember_repo::find_recent_by_prefix(
        pool,
        workshop_id,
        EmberType::Flare,
        BACKUP_FLARE_PREFIX,
        BACKUP_FLARE_COALESCE_SECS,
    )
    .await?;

    match existing {
        Some(ember) => {
            let count = parse_failure_count(&ember.content) + 1;
            let updated = format!("{BACKUP_FLARE_PREFIX}{error} ({count} failures in window)");
            ember_repo::update_content(pool, &ember.id, &updated).await?;
        }
        None => {
            ember_repo::create(
                pool,
                NewEmber {
                    ember_type: EmberType::Flare,
                    content: format!("{BACKUP_FLARE_PREFIX}{error}"),
                    source_agent: None,
                    workshop_id: workshop_id.to_string(),
                    ttl_seconds: Some(3600),
                },
            )
            .await?;
        }
    }
    Ok(())
}

fn parse_failure_count(content: &str) -> u32 {
    content
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(" failures in window)"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(1)
}
