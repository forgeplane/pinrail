-- A session's own name: an adjective and a chemical element, such as
-- Happy Hydrogen, given when the core first records the session, or the
-- name the person gave it. The element is its atomic number, which
-- places the session on the periodic table. The core fills both for the
-- sessions recorded before this migration, the first time it opens.
ALTER TABLE sessions ADD COLUMN element INTEGER;
ALTER TABLE sessions ADD COLUMN name TEXT;
ALTER TABLE sessions ADD COLUMN named_by TEXT NOT NULL DEFAULT 'pinrail'
  CHECK (named_by IN ('pinrail', 'person'));
CREATE INDEX sessions_element ON sessions(element);
