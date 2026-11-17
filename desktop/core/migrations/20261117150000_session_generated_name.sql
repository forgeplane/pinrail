-- The name Pinrail gave a session, kept when the person renames it, so
-- that clearing the person's name brings it back.
ALTER TABLE sessions ADD COLUMN generated_name TEXT;
UPDATE sessions SET generated_name = name WHERE named_by = 'pinrail';
