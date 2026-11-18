-- The name the person gave a session, or null. Without one, the session
-- goes by the name Pinrail derives from its id, such as Happy Hydrogen.
ALTER TABLE sessions ADD COLUMN name TEXT;
