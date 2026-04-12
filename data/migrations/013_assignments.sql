-- Assignments: actor-to-spark assignment with phase tracking, branch info,
-- and optimistic-concurrency event versioning.

CREATE TABLE IF NOT EXISTS assignments (
    assignment_id   TEXT PRIMARY KEY,
    spark_id        TEXT NOT NULL REFERENCES sparks(id) ON DELETE CASCADE,
    actor_id        TEXT NOT NULL,
    assignment_phase TEXT NOT NULL DEFAULT 'claimed',
    source_branch   TEXT,
    target_branch   TEXT,
    event_version   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_assignments_spark ON assignments(spark_id);
CREATE INDEX IF NOT EXISTS idx_assignments_actor ON assignments(actor_id);
CREATE INDEX IF NOT EXISTS idx_assignments_phase ON assignments(assignment_phase);
