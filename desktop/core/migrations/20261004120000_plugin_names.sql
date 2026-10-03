-- A plugin is known by its manifest's name alone. Every Pinrail so far is
-- a prerelease, so installations are not carried over: they are made
-- again, and the plugins the app carries are installed again at start.
DROP TABLE plugin_installs;

-- What the person has installed under each name, and where it came from.
CREATE TABLE plugin_installs (
  name         TEXT PRIMARY KEY,
  -- 'app' for a plugin the app carries, else a folder or a zip on disk
  source_kind  TEXT NOT NULL CHECK (source_kind IN ('app', 'folder', 'archive')),
  -- the folder or the zip as a full path; empty for 'app'
  source       TEXT NOT NULL,
  -- 1: the folder is followed, not copied
  link         INTEGER NOT NULL DEFAULT 0
               CHECK (link IN (0, 1) AND (link = 0 OR source_kind = 'folder')),
  -- the bundle new reviews use; none for a link, which is served from its folder
  bundle       TEXT REFERENCES plugin_bundles(hash),
  installed_at TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  CHECK (link = 1 OR bundle IS NOT NULL)
);

-- A review names its plugin the same way.
UPDATE reviews SET plugin = substr(plugin, instr(plugin, '/') + 1)
WHERE instr(plugin, '/') > 0;
