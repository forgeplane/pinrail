-- The newest status of an agent's session when the agent wrote the comment,
-- which places the comment in the session's thread: the times, in whole
-- seconds, do not always tell.
ALTER TABLE work_item_comments ADD COLUMN after_status INTEGER;
