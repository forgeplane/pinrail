-- A plugin is known by its publisher and its name, "forgeplane/list", and
-- each of its releases is a bundle on a compatibility line. Every Pinrail
-- so far is a prerelease, so installed plugins are not carried over: they
-- are installed again.
DROP TABLE installed_plugins;

-- What the person has installed, and where it comes from.
CREATE TABLE plugin_installs (
  plugin       TEXT PRIMARY KEY,
  publisher    TEXT NOT NULL,
  name         TEXT NOT NULL,
  source_kind  TEXT NOT NULL
               CHECK (source_kind IN ('bundled', 'folder', 'link', 'git', 'release')),
  -- as the person gave it, and as an install resolved it: a path, or the
  -- repository or release it was fetched from
  source       TEXT NOT NULL,
  resolved     TEXT NOT NULL,
  commit_id    TEXT,
  asset_hash   TEXT,
  build_log    TEXT,
  -- the line new reviews use; none for a link, which is served live
  line         TEXT,
  installed_at TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
-- an agent names a plugin by its name alone
CREATE INDEX plugin_installs_name ON plugin_installs(name);

-- Each line's current bundle. A line outlives its installation while
-- reviews use it.
CREATE TABLE plugin_lines (
  plugin TEXT NOT NULL,
  line   TEXT NOT NULL,
  bundle TEXT NOT NULL REFERENCES plugin_bundles(hash),
  PRIMARY KEY (plugin, line)
);
CREATE INDEX plugin_lines_bundle ON plugin_lines(bundle);

-- A review records the plugin's full name, the line it renders with, and
-- the exact version it was submitted to. The reviews kept from before
-- were made with the official plugins.
DROP INDEX reviews_plugin;
ALTER TABLE reviews RENAME COLUMN plugin_version TO plugin_major;
ALTER TABLE reviews RENAME COLUMN plugin_release TO plugin_version;
ALTER TABLE reviews ADD COLUMN plugin_line TEXT NOT NULL DEFAULT '';
UPDATE reviews SET plugin_version = coalesce(plugin_version, plugin_major || '.0.0');
UPDATE reviews SET plugin_line = CASE
  WHEN plugin_major > 0 THEN CAST(plugin_major AS TEXT)
  ELSE '0.' || substr(plugin_version, 3, instr(substr(plugin_version, 3), '.') - 1)
END;
UPDATE reviews SET plugin = 'forgeplane/' || plugin;
ALTER TABLE reviews DROP COLUMN plugin_major;
CREATE INDEX reviews_plugin ON reviews(plugin, plugin_line);
