-- When the person archived the session, or NULL. Archived sessions leave
-- the Sessions page and the sidebar, and new activity in a session clears
-- it. A nullable column, which ADD COLUMN appends without a rebuild.
ALTER TABLE sessions ADD COLUMN archived_at TEXT;
