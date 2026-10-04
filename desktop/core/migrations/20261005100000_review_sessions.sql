-- The agent's session a review was asked from, as the CLI found it, so a
-- review can be traced back to it.
ALTER TABLE reviews ADD COLUMN session TEXT;
