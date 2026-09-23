-- The schema as it stood on 2026-09-23, when migrations moved to the
-- schema_migrations table; the numbered steps before it are squashed here.

CREATE TABLE reviews (
  id             TEXT PRIMARY KEY,
  plugin         TEXT NOT NULL,
  plugin_version INTEGER NOT NULL,
  title          TEXT NOT NULL,
  origin         TEXT NOT NULL,
  requested_by   TEXT,
  payload        TEXT NOT NULL,
  summary        TEXT,
  revises        TEXT REFERENCES reviews(id),
  expires_at     TEXT,
  created_at     TEXT NOT NULL,
  -- the exact release it was submitted to; the major above is what renders it
  plugin_release TEXT
);
CREATE INDEX reviews_created ON reviews(created_at DESC);
CREATE INDEX reviews_revises ON reviews(revises);

CREATE TABLE events (
  id        INTEGER PRIMARY KEY,
  review_id TEXT REFERENCES reviews(id),
  kind      TEXT NOT NULL,
  actor     TEXT,
  at        TEXT NOT NULL,
  attrs     TEXT
);
CREATE INDEX events_review ON events(review_id, id);

-- how a review ended: once, whichever way
CREATE TABLE outcomes (
  review_id  TEXT PRIMARY KEY REFERENCES reviews(id),
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
