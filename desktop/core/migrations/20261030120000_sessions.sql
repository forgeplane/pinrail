-- An agent session: the agent and the session id it reported, what it
-- last reported about where it works, and when it was last active. A
-- review names its session by `requested_by` and `session`, which the core
-- keeps in step with this table.
CREATE TABLE sessions (
  agent    TEXT NOT NULL,
  id       TEXT NOT NULL,
  -- the project, the branch and the folder the agent last reported
  repo     TEXT,
  ref      TEXT,
  folder   TEXT,
  first_at TEXT NOT NULL,
  -- the session's latest activity, to the millisecond, which orders the
  -- sessions
  last_at  TEXT NOT NULL,
  PRIMARY KEY (agent, id)
);
CREATE INDEX sessions_last_at ON sessions(last_at);

-- What an agent said it is doing, with `pinrail status`, oldest first.
CREATE TABLE session_statuses (
  id         INTEGER PRIMARY KEY,
  agent      TEXT NOT NULL,
  session    TEXT NOT NULL,
  text       TEXT NOT NULL,
  -- whether the agent said it has finished
  done       INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1)),
  created_at TEXT NOT NULL,
  FOREIGN KEY (agent, session) REFERENCES sessions(agent, id) ON DELETE CASCADE
);
CREATE INDEX session_statuses_session ON session_statuses(agent, session, id);

-- the sessions the reviews already name, with the newest review's project
-- and branch, and their newest event other than a review being viewed, its
-- time written to the millisecond as new activity is
WITH named AS (
  SELECT r.requested_by AS agent, r.session AS id, r.origin, r.created_at,
    ROW_NUMBER() OVER (PARTITION BY r.requested_by, r.session ORDER BY r.created_at DESC, r.id DESC) AS newest,
    MIN(r.created_at) OVER (PARTITION BY r.requested_by, r.session) AS first_at
  FROM reviews r
  WHERE r.requested_by IS NOT NULL AND r.session IS NOT NULL
),
latest AS (
  SELECT r.requested_by AS agent, r.session AS id, MAX(e.at) AS at
  FROM events e JOIN reviews r ON r.id = e.review_id
  WHERE r.requested_by IS NOT NULL AND r.session IS NOT NULL AND e.kind <> 'viewed'
  GROUP BY r.requested_by, r.session
)
INSERT INTO sessions (agent, id, repo, ref, folder, first_at, last_at)
SELECT n.agent, n.id, json_extract(n.origin, '$.repo'), json_extract(n.origin, '$.ref'), NULL,
  n.first_at, strftime('%Y-%m-%dT%H:%M:%fZ', COALESCE(l.at, n.created_at))
FROM named n LEFT JOIN latest l ON l.agent = n.agent AND l.id = n.id
WHERE n.newest = 1;
