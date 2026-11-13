-- Whether the session is known only from its agent's hooks. Such a
-- session stays out of every list until it reports a status, asks a
-- question, submits a review or is started from Pinrail, and goes after 7
-- days without any of these. A flag, which ADD COLUMN appends.
ALTER TABLE sessions ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1));

-- What a session's agent is doing now, from the reports of its hooks: one
-- row per session that has reported, none for the others, whose state is
-- unknown. Kept apart from `sessions`, since a report arrives with every
-- tool call and most reads of `sessions` do not need it.
CREATE TABLE session_presence (
  agent        TEXT NOT NULL,
  session      TEXT NOT NULL,
  -- working, needs_input, idle, stopped
  state        TEXT NOT NULL,
  -- with needs_input: permission, question
  wait_kind    TEXT,
  -- with needs_input: the tool and a summary of the call, at most 200 characters
  wait_tool    TEXT,
  wait_summary TEXT,
  -- with stopped: ended, exited, cleared
  stop_reason  TEXT,
  -- where the state came from: hook, status (later: host, protocol, process)
  source       TEXT NOT NULL,
  -- the agent's process, which tells a stopped agent from a silent one
  pid          INTEGER,
  proc_started TEXT,
  -- the order of the reports: a report with a lower seq is older, and
  -- is ignored
  seq          INTEGER NOT NULL,
  state_since  TEXT NOT NULL,
  -- the turns: the number of the latest, when it began, and the latest
  -- the person has seen
  turn         INTEGER NOT NULL DEFAULT 0,
  turn_started TEXT,
  seen_turn    INTEGER NOT NULL DEFAULT 0,
  updated_at   TEXT NOT NULL,
  PRIMARY KEY (agent, session),
  FOREIGN KEY (agent, session) REFERENCES sessions (agent, id) ON DELETE CASCADE
);

-- One row per turn of a session, for its timeline: when it began and
-- ended, and how.
CREATE TABLE session_turns (
  id         INTEGER PRIMARY KEY,
  agent      TEXT NOT NULL,
  session    TEXT NOT NULL,
  turn       INTEGER NOT NULL,
  started_at TEXT NOT NULL,
  ended_at   TEXT,
  -- done, failed, interrupted, ended
  end_kind   TEXT,
  UNIQUE (agent, session, turn),
  FOREIGN KEY (agent, session) REFERENCES sessions (agent, id) ON DELETE CASCADE
);

-- The prompt and the reply of a turn: large, and read only by the thread.
CREATE TABLE session_turn_texts (
  turn_id INTEGER PRIMARY KEY REFERENCES session_turns (id) ON DELETE CASCADE,
  -- at most 4,000 characters
  prompt  TEXT,
  -- at most 8,000 characters
  reply   TEXT
);
