-- A link holds a bundle too, captured from its folder, so every
-- installation has one. A link made before has none and is dropped: link
-- the folder again.
CREATE TABLE plugin_installs_new (
  name         TEXT PRIMARY KEY,
  source_kind  TEXT NOT NULL CHECK (source_kind IN ('app', 'folder', 'archive')),
  source       TEXT NOT NULL,
  link         INTEGER NOT NULL DEFAULT 0
               CHECK (link IN (0, 1) AND (link = 0 OR source_kind = 'folder')),
  bundle       TEXT NOT NULL REFERENCES plugin_bundles(hash),
  installed_at TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
INSERT INTO plugin_installs_new
SELECT name, source_kind, source, link, bundle, installed_at, updated_at
FROM plugin_installs WHERE bundle IS NOT NULL;
DROP TABLE plugin_installs;
ALTER TABLE plugin_installs_new RENAME TO plugin_installs;
