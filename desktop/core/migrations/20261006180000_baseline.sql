-- The schema Pinrail 0.1.0 shipped with. Every change after it is a
-- migration of its own.

CREATE TABLE reviews (
  id             TEXT PRIMARY KEY,
  -- the plugin's name, as its manifest gives it
  plugin         TEXT NOT NULL,
  title          TEXT NOT NULL,
  origin         TEXT NOT NULL,
  requested_by   TEXT,
  -- the request summary, derived from the plugin's declaration at submit
  summary        TEXT,
  revises        TEXT REFERENCES reviews(id) ON DELETE SET NULL,
  expires_at     TEXT,
  created_at     TEXT NOT NULL,
  -- the plugin's version the review was submitted to
  plugin_version TEXT,
  -- the bundle the review renders and validates with
  plugin_bundle  TEXT REFERENCES plugin_bundles(hash),
  -- the agent's session the review was asked from, as the CLI found it
  session        TEXT
);
CREATE INDEX reviews_plugin ON reviews(plugin);
-- which reviews keep a bundle, for the sweep
CREATE INDEX reviews_plugin_bundle ON reviews(plugin_bundle);
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
  agent_note TEXT,
  -- the outcome summary, derived from the plugin's declaration when the
  -- review is decided
  summary    TEXT
);
CREATE INDEX outcomes_kind ON outcomes(kind);

-- Every plugin bundle the app holds, stored once under bundles/<hash>/
-- and never changed. stored_at is when it was last stored: the sweep
-- leaves a bundle stored within the hour, which an install may be about
-- to refer to.
CREATE TABLE plugin_bundles (
  hash      TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  version   TEXT NOT NULL,
  manifest  TEXT NOT NULL,
  size      INTEGER NOT NULL,
  stored_at TEXT NOT NULL
);

-- The listing a bundle's hash was taken over, which can run to thousands
-- of lines, in a table of its own.
CREATE TABLE plugin_bundle_files (
  hash    TEXT PRIMARY KEY REFERENCES plugin_bundles(hash) ON DELETE CASCADE,
  listing TEXT NOT NULL
);

-- What the person has installed under each name, and where it came from.
CREATE TABLE plugin_installs (
  name         TEXT PRIMARY KEY,
  -- an official plugin from a catalog, by its id; or a folder or a zip on disk
  source_kind  TEXT NOT NULL CHECK (source_kind IN ('index', 'folder', 'archive')),
  -- the catalog id, or the folder or the zip as a full path
  source       TEXT NOT NULL,
  -- 1: the folder is followed, not copied
  link         INTEGER NOT NULL DEFAULT 0
               CHECK (link IN (0, 1) AND (link = 0 OR source_kind = 'folder')),
  -- the bundle new reviews use; a link's is captured from its folder
  bundle       TEXT NOT NULL REFERENCES plugin_bundles(hash),
  installed_at TEXT NOT NULL,
  updated_at   TEXT NOT NULL
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
