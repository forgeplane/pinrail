-- The release a line's current one replaced, kept until `previous_until`
-- so an update can be rolled back; the sweep leaves it until then.
ALTER TABLE plugin_lines ADD COLUMN previous TEXT REFERENCES plugin_bundles(hash);
ALTER TABLE plugin_lines ADD COLUMN previous_until TEXT;
