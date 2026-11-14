-- The computers that run agents for Pinrail: this one, and later the ones
-- reached over SSH or the relay.
CREATE TABLE hosts (
  id           TEXT PRIMARY KEY,         -- 'local', or 'h_<ulid>'
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('local', 'ssh', 'relay')),
  -- the SSH destination, with kind ssh
  target       TEXT,
  -- the device's key, with kind relay
  public_key   TEXT,
  created_at   TEXT NOT NULL,
  last_seen_at TEXT
);
INSERT INTO hosts (id, name, kind, created_at)
  VALUES ('local', 'This computer', 'local', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

-- The Git worktrees agents work in: those an agent made with its own
-- option, and those Pinrail made for it.
CREATE TABLE worktrees (
  id         INTEGER PRIMARY KEY,
  host_id    TEXT NOT NULL REFERENCES hosts (id) ON DELETE CASCADE,
  repo_root  TEXT NOT NULL,
  path       TEXT NOT NULL,
  branch     TEXT NOT NULL,
  base       TEXT,
  created_at TEXT NOT NULL,
  removed_at TEXT,
  owner      TEXT NOT NULL CHECK (owner IN ('agent', 'pinrail')),
  UNIQUE (host_id, path)
);

-- Each agent process Pinrail starts in a terminal of a host. A run is
-- written as starting before the host is asked to spawn it, so a spawn
-- whose reply was lost is found by its id and never started twice.
CREATE TABLE runs (
  id           TEXT PRIMARY KEY,         -- 'run_<ulid>'
  host_id      TEXT NOT NULL REFERENCES hosts (id) ON DELETE CASCADE,
  agent        TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('start', 'resume')),
  -- the host's terminal, once it spawned the run
  terminal_id  TEXT,
  -- the run's own agent process, and its start time as pinrail-runtime
  -- reads it: only this process may speak with the run's token
  pid          INTEGER,
  proc_started TEXT,
  folder       TEXT NOT NULL,
  worktree_id  INTEGER REFERENCES worktrees (id) ON DELETE SET NULL,
  state        TEXT NOT NULL CHECK (state IN ('starting', 'running', 'ended')),
  -- exited, stopped, host_lost, restart, start_failed, resume_failed
  end_reason   TEXT,
  exit_code    INTEGER,
  exit_signal  TEXT,
  created_by   TEXT NOT NULL CHECK (created_by IN ('app', 'cli')),
  started_at   TEXT NOT NULL,
  ended_at     TEXT
);
CREATE INDEX runs_live ON runs (state) WHERE state != 'ended';

-- The command a run started with, kept apart because a first task can be
-- long.
CREATE TABLE run_launches (
  run_id TEXT PRIMARY KEY REFERENCES runs (id) ON DELETE CASCADE,
  -- a JSON array
  argv   TEXT NOT NULL
);

-- The sessions a run has been in, in order: clearing or forking moves a
-- run to a new session.
CREATE TABLE run_sessions (
  run_id     TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
  agent      TEXT NOT NULL,
  session    TEXT NOT NULL,
  bound_at   TEXT NOT NULL,
  unbound_at TEXT,
  PRIMARY KEY (run_id, agent, session),
  FOREIGN KEY (agent, session) REFERENCES sessions (agent, id) ON DELETE CASCADE
);
CREATE INDEX run_sessions_session ON run_sessions (agent, session);

-- The SHA-256 digest of each run's token. The token itself is never
-- stored. A run keeps its token in its environment across a restart of
-- the core, so the digest outlives the core, and it goes 5 minutes after
-- the run ends, so that late reports of the exiting agent still count.
CREATE TABLE run_tokens (
  run_id TEXT PRIMARY KEY REFERENCES runs (id) ON DELETE CASCADE,
  digest TEXT NOT NULL UNIQUE
);

-- The host a session runs on, NULL for this computer. Nullable foreign
-- keys, which ADD COLUMN allows.
ALTER TABLE sessions ADD COLUMN host_id TEXT REFERENCES hosts (id);
ALTER TABLE session_presence ADD COLUMN run_id TEXT REFERENCES runs (id) ON DELETE SET NULL;
ALTER TABLE session_turns ADD COLUMN run_id TEXT REFERENCES runs (id) ON DELETE SET NULL;
