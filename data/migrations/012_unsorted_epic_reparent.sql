-- Reparent orphan sparks under a per-workshop 'Unsorted' catch-all epic.
--
-- Prerequisite for the no-orphan invariant landing on create_spark: without
-- this migration, any orphan non-epic sparks that exist in pre-invariant DBs
-- would become un-queryable (or invariant-violating) the moment the invariant
-- ships. Here we create exactly one 'Unsorted' epic per workshop that has at
-- least one orphan non-epic spark, then reparent every such orphan under it.
--
-- Idempotency: the synthetic epic's id is deterministic
-- (`<workshop_id>-unsorted-epic`) and the insert uses `INSERT OR IGNORE`, so
-- re-executing this SQL produces no duplicate epics. The UPDATE only touches
-- rows that are still orphaned (`parent_id IS NULL`), so already-reparented
-- sparks are left alone. No row is ever deleted.

INSERT OR IGNORE INTO sparks (
    id,
    title,
    description,
    status,
    priority,
    spark_type,
    workshop_id,
    metadata,
    created_at,
    updated_at
)
SELECT
    workshop_id || '-unsorted-epic',
    'Unsorted',
    'Migration-created catch-all epic. Parent for sparks that existed before the no-orphan invariant was enforced; reparent or re-home them as needed.',
    'open',
    4,
    'epic',
    workshop_id,
    '{}',
    '2026-04-09T00:00:00Z',
    '2026-04-09T00:00:00Z'
FROM (
    SELECT DISTINCT workshop_id
    FROM sparks
    WHERE parent_id IS NULL
      AND spark_type != 'epic'
) AS orphan_workshops;

UPDATE sparks
SET parent_id = workshop_id || '-unsorted-epic',
    updated_at = '2026-04-09T00:00:00Z'
WHERE parent_id IS NULL
  AND spark_type != 'epic';
