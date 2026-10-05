-- An official plugin is installed from a catalog, the app's or later the
-- registry's, by its id: source_kind 'index', with the id as its source.
-- The app no longer installs its own copies, so 'app' goes; a copy it
-- installed before becomes an install of the official plugin by that name.
CREATE TABLE plugin_installs_new (
  name         TEXT PRIMARY KEY,
  source_kind  TEXT NOT NULL CHECK (source_kind IN ('index', 'folder', 'archive')),
  source       TEXT NOT NULL,
  link         INTEGER NOT NULL DEFAULT 0
               CHECK (link IN (0, 1) AND (link = 0 OR source_kind = 'folder')),
  bundle       TEXT NOT NULL REFERENCES plugin_bundles(hash),
  installed_at TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
INSERT INTO plugin_installs_new
  (name, source_kind, source, link, bundle, installed_at, updated_at)
SELECT name,
       CASE source_kind WHEN 'app' THEN 'index' ELSE source_kind END,
       CASE source_kind WHEN 'app' THEN 'forgeplane/' || name ELSE source END,
       link, bundle, installed_at, updated_at
FROM plugin_installs;
DROP TABLE plugin_installs;
ALTER TABLE plugin_installs_new RENAME TO plugin_installs;
