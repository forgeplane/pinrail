-- A review's summaries are derived from its plugin's declaration: the
-- request summary into reviews.summary at submit, the outcome summary
-- here when the review is decided. What agents sent as a summary before
-- is not in that shape, so it is dropped.
ALTER TABLE outcomes ADD COLUMN summary TEXT;
UPDATE reviews SET summary = NULL;
