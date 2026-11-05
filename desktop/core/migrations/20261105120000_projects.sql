-- A set of repositories the person named, to group the inbox and the
-- sidebar by. Projects have no stored order: the app orders them by what
-- waits in them and by their latest activity.
CREATE TABLE projects (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  created_at TEXT NOT NULL
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
