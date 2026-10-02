-- Every plugin bundle the app holds, stored once under bundles/<hash>/
-- and never changed. stored_at is when it was last stored: the sweep
-- leaves a bundle stored within the hour, which an install may be about
-- to refer to.
CREATE TABLE plugin_bundles (
  hash      TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  version   TEXT NOT NULL,
  line      TEXT NOT NULL,
  manifest  TEXT NOT NULL,
  size      INTEGER NOT NULL,
  stored_at TEXT NOT NULL
);

-- The listing a bundle's hash was taken over, which can run to thousands
-- of lines, in a table of its own.
CREATE TABLE plugin_bundle_files (
  hash    TEXT PRIMARY KEY REFERENCES plugin_bundles(hash) ON DELETE CASCADE,
  listing TEXT NOT NULL
);
