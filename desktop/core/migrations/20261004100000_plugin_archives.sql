-- A plugin can be installed from a zip on disk. SQLite cannot change a
-- CHECK, so the table is made again with the same rows.
CREATE TABLE plugin_installs_new (
  plugin         TEXT PRIMARY KEY,
  publisher      TEXT NOT NULL,
  name           TEXT NOT NULL,
  source_kind    TEXT NOT NULL
                 CHECK (source_kind IN ('bundled', 'folder', 'archive', 'link', 'git', 'release')),
  source         TEXT NOT NULL,
  resolved       TEXT NOT NULL,
  commit_id      TEXT,
  asset_hash     TEXT,
  build_log      TEXT,
  installed_at   TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  bundle         TEXT REFERENCES plugin_bundles(hash),
  previous       TEXT REFERENCES plugin_bundles(hash),
  previous_until TEXT,
  replaced       TEXT
);
INSERT INTO plugin_installs_new
  (plugin, publisher, name, source_kind, source, resolved, commit_id, asset_hash,
   build_log, installed_at, updated_at, bundle, previous, previous_until, replaced)
SELECT plugin, publisher, name, source_kind, source, resolved, commit_id, asset_hash,
       build_log, installed_at, updated_at, bundle, previous, previous_until, replaced
FROM plugin_installs;
DROP TABLE plugin_installs;
ALTER TABLE plugin_installs_new RENAME TO plugin_installs;
CREATE INDEX plugin_installs_name ON plugin_installs(name);
