-- Which files a note carries, by the names its Markdown refers to them by.
-- The bytes are the review attachments' blobs: a file a review and a note
-- both send is one blob, kept while either names it.
CREATE TABLE status_attachments (
  status_id  INTEGER NOT NULL REFERENCES session_statuses(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  sha256     TEXT NOT NULL REFERENCES blobs(sha256),
  size       INTEGER NOT NULL,
  media_type TEXT NOT NULL,
  PRIMARY KEY (status_id, name)
);
CREATE INDEX status_attachments_sha256 ON status_attachments(sha256);
