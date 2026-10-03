-- A link that takes a published plugin's place keeps that installation
-- here, as JSON, and puts it back when the link is removed.
ALTER TABLE plugin_installs ADD COLUMN replaced TEXT;
