-- Releases: foundation for the release-planning workflow.
--
-- A `release` bundles one or more epic sparks into a shippable unit with its
-- own lifecycle. `release_epics` is the many-to-many link between a release
-- and the sparks it includes.
--
-- Invariant: a single spark cannot belong to more than one release while that
-- other release is still in an "open" state (planning|in_progress|ready).
--
-- Spark ryve-d5032784 [sp-2a82fee7].

CREATE TABLE IF NOT EXISTS releases (
    id               TEXT PRIMARY KEY,
    version          TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'planning'
                     CHECK (status IN ('planning','in_progress','ready','cut','closed','abandoned')),
    branch_name      TEXT,
    created_at       TEXT NOT NULL,
    cut_at            TEXT,
    tag              TEXT,
    artifact_path    TEXT,
    problem          TEXT,
    acceptance_json  TEXT NOT NULL DEFAULT '[]',
    notes            TEXT
);

CREATE INDEX IF NOT EXISTS idx_releases_status ON releases(status);
CREATE INDEX IF NOT EXISTS idx_releases_version ON releases(version);

CREATE TABLE IF NOT EXISTS release_epics (
    release_id  TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    spark_id    TEXT NOT NULL REFERENCES sparks(id) ON DELETE CASCADE,
    added_at    TEXT NOT NULL,
    PRIMARY KEY (release_id, spark_id)
);

CREATE INDEX IF NOT EXISTS idx_release_epics_spark ON release_epics(spark_id);

CREATE TRIGGER IF NOT EXISTS release_epics_single_open_insert
BEFORE INSERT ON release_epics
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM release_epics re
    JOIN releases r ON r.id = re.release_id
    WHERE re.spark_id = NEW.spark_id
      AND re.release_id != NEW.release_id
      AND r.status IN ('planning','in_progress','ready')
)
BEGIN
    SELECT RAISE(ABORT, 'release_epic conflict: spark already belongs to another open release');
END;

CREATE TRIGGER IF NOT EXISTS release_reopen_conflict_check
BEFORE UPDATE OF status ON releases
FOR EACH ROW
WHEN NEW.status IN ('planning','in_progress','ready')
     AND OLD.status NOT IN ('planning','in_progress','ready')
     AND EXISTS (
         SELECT 1
         FROM release_epics re1
         JOIN release_epics re2
             ON re1.spark_id = re2.spark_id
            AND re1.release_id != re2.release_id
         JOIN releases r2 ON r2.id = re2.release_id
         WHERE re1.release_id = NEW.id
           AND r2.status IN ('planning','in_progress','ready')
     )
BEGIN
    SELECT RAISE(ABORT, 'release_status conflict: reopening would duplicate an epic in another open release');
END;
