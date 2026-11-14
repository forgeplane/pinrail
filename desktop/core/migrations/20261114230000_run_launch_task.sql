-- The first task and the extra arguments a run was started with, so the
-- person can start it again as it was, or edit them first.
ALTER TABLE run_launches ADD COLUMN task TEXT;
ALTER TABLE run_launches ADD COLUMN args TEXT NOT NULL DEFAULT '[]';
