-- The time the person starred a round, or NULL. A review shows as starred
-- when any of its rounds is. A nullable column, which ADD COLUMN appends
-- without rebuilding the table.
ALTER TABLE reviews ADD COLUMN starred_at TEXT;
-- only the starred rounds, so it costs nothing for the others
CREATE INDEX reviews_starred_at ON reviews(starred_at) WHERE starred_at IS NOT NULL;
