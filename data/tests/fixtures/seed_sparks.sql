-- Seed data for tests
INSERT INTO sparks (id, title, description, status, priority, spark_type, workshop_id, created_at, updated_at)
VALUES
    ('sp-0001', 'Setup CI pipeline', 'Configure GitHub Actions', 'open', 1, 'task', 'ws-test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
    ('sp-0002', 'Fix login bug', 'Users cant login with SSO', 'open', 0, 'bug', 'ws-test', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z'),
    ('sp-0003', 'Add dark mode', 'Theme support', 'open', 2, 'feature', 'ws-test', '2026-01-03T00:00:00Z', '2026-01-03T00:00:00Z'),
    ('sp-0004', 'Write tests', 'Unit tests for auth', 'in_progress', 1, 'task', 'ws-test', '2026-01-04T00:00:00Z', '2026-01-04T00:00:00Z'),
    ('sp-0005', 'Deploy v1', 'Production release', 'closed', 2, 'milestone', 'ws-test', '2026-01-05T00:00:00Z', '2026-01-05T00:00:00Z');

-- sp-0002 blocks sp-0003 (can't add dark mode until login bug is fixed)
INSERT INTO bonds (from_id, to_id, bond_type) VALUES ('sp-0002', 'sp-0003', 'blocks');
-- sp-0001 is parent of sp-0004
INSERT INTO bonds (from_id, to_id, bond_type) VALUES ('sp-0001', 'sp-0004', 'parent_child');

INSERT INTO stamps (spark_id, name) VALUES ('sp-0001', 'infra');
INSERT INTO stamps (spark_id, name) VALUES ('sp-0002', 'bug');
INSERT INTO stamps (spark_id, name) VALUES ('sp-0002', 'auth');

INSERT INTO comments (id, spark_id, author, body, created_at)
VALUES
    ('cm-00000001', 'sp-0001', 'alice', 'Should we use GitHub Actions or CircleCI?', '2026-01-01T12:00:00Z'),
    ('cm-00000002', 'sp-0001', 'bob', 'GitHub Actions is fine', '2026-01-01T13:00:00Z');

INSERT INTO embers (id, ember_type, content, source_agent, workshop_id, ttl_seconds, created_at)
VALUES ('em-00000001', 'flash', 'API interface changed', 'agent-1', 'ws-test', 3600, '2026-04-04T00:00:00Z');

INSERT INTO engravings (key, workshop_id, value, author, created_at, updated_at)
VALUES ('auth_pattern', 'ws-test', 'JWT middleware in src/auth/', 'agent-1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
