-- The newest status of the session when the message was sent, or NULL,
-- which places the message among the statuses: their times, in whole
-- seconds, do not always tell. A nullable column, which ADD COLUMN appends
-- without a rebuild.
ALTER TABLE session_messages ADD COLUMN after_status INTEGER;
