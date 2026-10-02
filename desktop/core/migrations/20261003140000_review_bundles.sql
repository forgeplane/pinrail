-- A review renders and validates with the bundle it was submitted to, and
-- an installation names the bundle new reviews use and the one before it,
-- kept for a rollback. Plugin lines go.
ALTER TABLE reviews ADD COLUMN plugin_bundle TEXT REFERENCES plugin_bundles(hash);
UPDATE reviews SET plugin_bundle = (
  SELECT l.bundle FROM plugin_lines l
  WHERE l.plugin = reviews.plugin AND l.line = reviews.plugin_line
);
DROP INDEX reviews_plugin;
ALTER TABLE reviews DROP COLUMN plugin_line;
CREATE INDEX reviews_plugin ON reviews(plugin);
-- which reviews keep a bundle, for the sweep
CREATE INDEX reviews_plugin_bundle ON reviews(plugin_bundle);

ALTER TABLE plugin_installs ADD COLUMN bundle TEXT REFERENCES plugin_bundles(hash);
ALTER TABLE plugin_installs ADD COLUMN previous TEXT REFERENCES plugin_bundles(hash);
ALTER TABLE plugin_installs ADD COLUMN previous_until TEXT;
UPDATE plugin_installs SET
  bundle = (SELECT l.bundle FROM plugin_lines l
            WHERE l.plugin = plugin_installs.plugin AND l.line = plugin_installs.line),
  previous = (SELECT l.previous FROM plugin_lines l
              WHERE l.plugin = plugin_installs.plugin AND l.line = plugin_installs.line),
  previous_until = (SELECT l.previous_until FROM plugin_lines l
                    WHERE l.plugin = plugin_installs.plugin AND l.line = plugin_installs.line);
ALTER TABLE plugin_installs DROP COLUMN line;
DROP TABLE plugin_lines;
ALTER TABLE plugin_bundles DROP COLUMN line;
