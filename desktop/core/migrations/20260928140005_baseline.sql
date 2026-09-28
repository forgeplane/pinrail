-- The schema Pinrail 0.1.0 shipped with. Every change after it is a
-- migration of its own.

CREATE TABLE reviews (
  id             TEXT PRIMARY KEY,
  plugin         TEXT NOT NULL,
  plugin_version INTEGER NOT NULL,
  title          TEXT NOT NULL,
  origin         TEXT NOT NULL,
  requested_by   TEXT,
  summary        TEXT,
  revises        TEXT REFERENCES reviews(id) ON DELETE SET NULL,
  expires_at     TEXT,
  created_at     TEXT NOT NULL,
  -- the exact release it was submitted to; the major above is what renders it
  plugin_release TEXT
);
-- which reviews render from a plugin's major, before its entry is removed
CREATE INDEX reviews_plugin ON reviews(plugin, plugin_version);
-- A round has at most one newer round, so the rounds of a review form a
-- single line. SQLite allows any number of NULLs in a unique index.
CREATE UNIQUE INDEX reviews_revises ON reviews(revises);

-- A review's payload, kept apart from its row: a payload can run to
-- megabytes, and listing reviews never reads it.
CREATE TABLE review_payloads (
  review_id TEXT PRIMARY KEY REFERENCES reviews(id) ON DELETE CASCADE,
  payload   TEXT NOT NULL
);

CREATE TABLE events (
  id        INTEGER PRIMARY KEY,
  review_id TEXT REFERENCES reviews(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL,
  actor     TEXT,
  at        TEXT NOT NULL,
  attrs     TEXT
);
CREATE INDEX events_review ON events(review_id, id);

-- how a review ended: once, whichever way
CREATE TABLE outcomes (
  review_id  TEXT PRIMARY KEY REFERENCES reviews(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('decided', 'withdrawn', 'discarded')),
  at         TEXT NOT NULL,
  by         TEXT,
  reason     TEXT,
  data       TEXT,
  agent_note TEXT
);
CREATE INDEX outcomes_kind ON outcomes(kind);

CREATE TABLE installed_plugins (
  name         TEXT PRIMARY KEY,
  version      TEXT NOT NULL,
  major        INTEGER NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('path', 'git', 'release')),
  source       TEXT NOT NULL,
  resolved     TEXT NOT NULL,
  commit_id    TEXT,
  asset_hash   TEXT,
  hash         TEXT,
  build_log    TEXT,
  installed_at TEXT NOT NULL,
  linked       INTEGER NOT NULL DEFAULT 0,
  path         TEXT NOT NULL
);

-- Files uploaded beside reviews, one row per distinct content. The bytes
-- live on disk under <data>/attachments/sha256/, named by their hash.
CREATE TABLE blobs (
  sha256     TEXT PRIMARY KEY,
  size       INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

-- Which files a review carries, by the names its payload refers to them by.
-- A blob no row names any more is swept once it is an hour old.
CREATE TABLE review_attachments (
  review_id  TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  sha256     TEXT NOT NULL REFERENCES blobs(sha256),
  size       INTEGER NOT NULL,
  media_type TEXT NOT NULL,
  PRIMARY KEY (review_id, name)
);
CREATE INDEX review_attachments_sha256 ON review_attachments(sha256);
