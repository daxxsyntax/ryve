-- Assignment table: links an actor to a spark with a phase lifecycle.
-- Phase values are stored as TEXT and validated by the Rust AssignmentPhase enum.

CREATE TABLE IF NOT EXISTS assignments (
    assignment_id       TEXT PRIMARY KEY NOT NULL,
    spark_id            TEXT NOT NULL REFERENCES sparks(id) ON DELETE RESTRICT,
    actor_id            TEXT NOT NULL,
    assignment_phase    TEXT NOT NULL,
    source_branch       TEXT,
    target_branch       TEXT,
    event_version       INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_assignments_spark ON assignments(spark_id);
CREATE INDEX IF NOT EXISTS idx_assignments_actor ON assignments(actor_id);
CREATE INDEX IF NOT EXISTS idx_assignments_phase ON assignments(assignment_phase);
