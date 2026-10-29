-- A review is a plugin's review, or a question an agent asks with
-- `pinrail ask`, which the app shows with a form of its own. A question has
-- no plugin: its name is the empty string, which no plugin can have.
ALTER TABLE reviews ADD COLUMN kind TEXT NOT NULL DEFAULT 'review'
  CHECK (kind IN ('review', 'question') AND (kind = 'question') = (plugin = ''));
-- a session's questions and reviews, in order, for the session view
CREATE INDEX reviews_session ON reviews(requested_by, session, created_at);
