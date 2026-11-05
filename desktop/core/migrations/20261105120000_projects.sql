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

-- Which project a repository belongs to, by the repository as the origins
-- name it, such as acme/api. A repository is in at most one project, so
-- it is the key; deleting a project takes its repositories out of it.
CREATE TABLE project_repos (
  repo       TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  added_at   TEXT NOT NULL
);
CREATE INDEX project_repos_project ON project_repos(project_id);

-- Filtering by a project reads the reviews' and the sessions' repository.
CREATE INDEX reviews_repo ON reviews(json_extract(origin, '$.repo'));
CREATE INDEX sessions_repo ON sessions(repo);
