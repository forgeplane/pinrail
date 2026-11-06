-- Work items: tasks the person and agents record, break down and work
-- through. An item belongs to a repository, which numbers its items; its
-- key is the repository's prefix and that number, such as PIN-42.
CREATE TABLE work_items (
  id              TEXT PRIMARY KEY,
  repo_id         INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  number          INTEGER NOT NULL,
  title           TEXT NOT NULL,
  -- Markdown: the goal, the context and what done means
  body            TEXT NOT NULL DEFAULT '',
  status          TEXT NOT NULL CHECK (status IN ('open', 'in_progress', 'done', 'cancelled')),
  -- the item this one is part of; deleting it deletes its sub-items
  parent          TEXT REFERENCES work_items(id) ON DELETE CASCADE,
  link            TEXT,
  -- the agent and session that hold the claim, while the item is in progress
  claim_agent     TEXT,
  claim_session   TEXT,
  -- 'person', or the agent's name, with its session
  created_by      TEXT NOT NULL,
  created_session TEXT,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  -- set when the item is done or cancelled, cleared when it opens again
  closed_at       TEXT,
  UNIQUE (repo_id, number)
);
CREATE INDEX work_items_parent ON work_items(parent);
CREATE INDEX work_items_repo_status ON work_items(repo_id, status);

-- The numbers an item had in other repositories before it moved, so that
-- its old keys still resolve.
CREATE TABLE work_item_aliases (
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  number  INTEGER NOT NULL,
  item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  PRIMARY KEY (repo_id, number)
);

-- An item depends on another: work on it should not start until the other
-- is done. The core refuses a cycle.
CREATE TABLE work_item_deps (
  item_id    TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  depends_on TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  PRIMARY KEY (item_id, depends_on),
  CHECK (item_id <> depends_on)
);
CREATE INDEX work_item_deps_reverse ON work_item_deps(depends_on);

-- What happened to an item, for its timeline: created, edited, moved, a
-- dependency, a claim, a status, a file, a review's end. The core checks
-- the kinds, so a new one needs no migration.
CREATE TABLE work_item_events (
  id      INTEGER PRIMARY KEY,
  item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL,
  -- 'person', or the agent's name, with its session
  actor   TEXT NOT NULL,
  session TEXT,
  at      TEXT NOT NULL,
  attrs   TEXT
);
CREATE INDEX work_item_events_item ON work_item_events(item_id, id);

-- The person's and agents' comments on an item, in Markdown.
CREATE TABLE work_item_comments (
  id         TEXT PRIMARY KEY,
  item_id    TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  body       TEXT NOT NULL,
  -- 'person', or the agent's name, with its session
  author     TEXT NOT NULL,
  session    TEXT,
  created_at TEXT NOT NULL,
  edited_at  TEXT
);
CREATE INDEX work_item_comments_item ON work_item_comments(item_id, created_at);

-- The files of an item's body and of its comments, stored once in blobs.
CREATE TABLE work_item_files (
  item_id    TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  -- the comment the file came with, or NULL for the body's files
  comment_id TEXT REFERENCES work_item_comments(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  sha256     TEXT NOT NULL REFERENCES blobs(sha256),
  size       INTEGER NOT NULL,
  media_type TEXT NOT NULL,
  PRIMARY KEY (item_id, name)
);

-- A review or a question can belong to one item; deleting the item leaves
-- the review, without one.
ALTER TABLE reviews ADD COLUMN item_id TEXT REFERENCES work_items(id) ON DELETE SET NULL;
CREATE INDEX reviews_item ON reviews(item_id);
