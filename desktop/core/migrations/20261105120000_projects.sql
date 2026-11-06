-- A set of repositories the person named, to group the inbox and the
-- sidebar by, with a line on what it is. Projects have no stored order: the
-- app lists them by name.
CREATE TABLE projects (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  -- a line on what the project is, if the person wrote one
  description TEXT,
  -- its avatar: a colour of the app's palette, by name, and an emoji or a
  -- letter or two in place of the name's first letter; the app picks the
  -- colour and shows the letter when they are NULL
  color       TEXT,
  emoji       TEXT,
  -- a PNG the person chose for the avatar, stored as a blob; the sweep
  -- leaves a blob a project names
  image       TEXT REFERENCES blobs(sha256),
  created_at  TEXT NOT NULL
);
-- two projects cannot share a name, whatever its case
CREATE UNIQUE INDEX projects_name ON projects(name COLLATE NOCASE);

-- A repository, by the name the origins give it, such as acme/api. Reviews
-- and sessions refer to their repository's row, which the core adds the
-- first time a name is used, so deleting the row deletes them. A
-- repository is in at most one project; deleting the project takes it out.
CREATE TABLE repos (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  -- the prefix of its work items' keys, set with its first item
  prefix     TEXT UNIQUE,
  -- the last number its work items were given, so that none is used twice
  last_item  INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX repos_project ON repos(project_id);

-- The prefixes a repository had before, so that its old keys still resolve.
CREATE TABLE repo_old_prefixes (
  prefix  TEXT PRIMARY KEY,
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE
);

-- The repositories the reviews and the sessions already name.
INSERT INTO repos (name, created_at)
SELECT name, MIN(at) FROM (
  SELECT json_extract(origin, '$.repo') AS name, created_at AS at FROM reviews
  UNION ALL
  SELECT repo, first_at FROM sessions
)
WHERE COALESCE(name, '') <> ''
GROUP BY name;

ALTER TABLE reviews ADD COLUMN repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE;
UPDATE reviews SET repo_id =
  (SELECT id FROM repos WHERE name = json_extract(reviews.origin, '$.repo'));
CREATE INDEX reviews_repo_id ON reviews(repo_id);

-- a session's repository is the one it last reported
ALTER TABLE sessions ADD COLUMN repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE;
UPDATE sessions SET repo_id = (SELECT id FROM repos WHERE name = sessions.repo);
ALTER TABLE sessions DROP COLUMN repo;
CREATE INDEX sessions_repo_id ON sessions(repo_id);
