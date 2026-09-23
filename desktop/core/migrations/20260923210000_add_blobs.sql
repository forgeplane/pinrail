-- Files uploaded beside reviews, one row per distinct content. The bytes
-- live on disk under <data>/artifacts/sha256/, named by their hash.
CREATE TABLE blobs (
  sha256     TEXT PRIMARY KEY,
  size       INTEGER NOT NULL,
  created_at TEXT NOT NULL
);
