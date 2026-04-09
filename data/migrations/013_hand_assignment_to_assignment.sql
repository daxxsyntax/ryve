-- Migration: HandAssignment → Assignment
-- spark: ryve-03dd980e
--
-- This migration creates the new `assignments` table and copies all existing
-- `hand_assignments` rows into it. The old table is then replaced with a
-- backward-compatible view so any code still referencing `hand_assignments`
-- continues to work.
--
-- Default values filled in for new columns:
--   phase       — derived from hand_assignments.status:
--                  'active'     → 'assigned'   (the Hand has claimed the spark)
--                  'completed'  → 'merged'     (terminal — work was completed)
--                  'handed_off' → 'assigned'   (re-assigned to another Hand)
--                  'abandoned'  → 'abandoned'  (terminal — Hand gave up)
--                  'expired'    → 'expired'    (terminal — heartbeat timeout)
--                  anything else→ 'assigned'   (safe fallback)
--   event_version — 0 for all migrated rows (no lifecycle events existed yet)
--   source_branch — 'hand/' || substr(session_id, 1, 8), matching the
--                   convention in workshop::create_hand_worktree
--   target_branch — 'main' (the default merge target for all Hand work)

-- ── Step 1: Create the assignments table ─────────────────

CREATE TABLE IF NOT EXISTS assignments (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id          TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    spark_id            TEXT NOT NULL REFERENCES sparks(id) ON DELETE CASCADE,
    status              TEXT NOT NULL DEFAULT 'active',
    role                TEXT NOT NULL DEFAULT 'owner',
    phase               TEXT NOT NULL DEFAULT 'assigned',
    event_version       INTEGER NOT NULL DEFAULT 0,
    source_branch       TEXT,
    target_branch       TEXT,
    assigned_at         TEXT NOT NULL,
    last_heartbeat_at   TEXT,
    lease_expires_at    TEXT,
    completed_at        TEXT,
    handoff_to          TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    handoff_reason      TEXT,
    UNIQUE(session_id, spark_id)
);

CREATE INDEX IF NOT EXISTS idx_assignments_session ON assignments(session_id);
CREATE INDEX IF NOT EXISTS idx_assignments_spark ON assignments(spark_id);
CREATE INDEX IF NOT EXISTS idx_assignments_status ON assignments(status);
CREATE INDEX IF NOT EXISTS idx_assignments_phase ON assignments(phase);

-- ── Step 2: Migrate data (idempotent — skips existing rows) ──────────

INSERT OR IGNORE INTO assignments (
    id,
    session_id,
    spark_id,
    status,
    role,
    phase,
    event_version,
    source_branch,
    target_branch,
    assigned_at,
    last_heartbeat_at,
    lease_expires_at,
    completed_at,
    handoff_to,
    handoff_reason
)
SELECT
    id,
    session_id,
    spark_id,
    status,
    role,
    CASE status
        WHEN 'active'     THEN 'assigned'
        WHEN 'completed'  THEN 'merged'
        WHEN 'handed_off' THEN 'assigned'
        WHEN 'abandoned'  THEN 'abandoned'
        WHEN 'expired'    THEN 'expired'
        ELSE 'assigned'
    END,
    0,
    'hand/' || substr(session_id, 1, 8),
    'main',
    assigned_at,
    last_heartbeat_at,
    lease_expires_at,
    completed_at,
    handoff_to,
    handoff_reason
FROM hand_assignments;

-- ── Step 3: Replace old table with a backward-compatible view ────────
-- Drop both the table (first run) and the view (re-run) so this step
-- is idempotent. SQLite requires DROP VIEW for views and DROP TABLE for
-- tables — issuing both with IF EXISTS covers both cases.

DROP TABLE IF EXISTS hand_assignments;

CREATE VIEW IF NOT EXISTS hand_assignments AS
SELECT
    id,
    session_id,
    spark_id,
    status,
    role,
    assigned_at,
    last_heartbeat_at,
    lease_expires_at,
    completed_at,
    handoff_to,
    handoff_reason
FROM assignments;
