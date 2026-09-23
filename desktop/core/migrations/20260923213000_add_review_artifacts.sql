-- Which files a review carries, by the names its payload refers to them by.
-- A blob no row names any more is swept once it is an hour old.
CREATE TABLE review_artifacts (
  review_id  TEXT NOT NULL REFERENCES reviews(id),
  name       TEXT NOT NULL,
  sha256     TEXT NOT NULL REFERENCES blobs(sha256),
  size       INTEGER NOT NULL,
  media_type TEXT NOT NULL,
  PRIMARY KEY (review_id, name)
);
CREATE INDEX review_artifacts_sha256 ON review_artifacts(sha256);
