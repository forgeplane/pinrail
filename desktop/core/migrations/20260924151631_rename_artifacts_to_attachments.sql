-- The files a review carries are attachments now.
ALTER TABLE review_artifacts RENAME TO review_attachments;
DROP INDEX review_artifacts_sha256;
CREATE INDEX review_attachments_sha256 ON review_attachments(sha256);
