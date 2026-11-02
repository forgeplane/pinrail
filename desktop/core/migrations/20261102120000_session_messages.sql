-- Messages the person sends to the agent from its session's thread. A
-- message is queued until the agent reports a status, which hands it over
-- and sets delivered_at; only a queued message can be deleted.
CREATE TABLE session_messages (
  id           INTEGER PRIMARY KEY,
  agent        TEXT NOT NULL,
  session      TEXT NOT NULL,
  text         TEXT NOT NULL,
  sent_by      TEXT NOT NULL,
  sent_at      TEXT NOT NULL,
  delivered_at TEXT,
  FOREIGN KEY (agent, session) REFERENCES sessions (agent, id) ON DELETE CASCADE
);

CREATE INDEX session_messages_queued ON session_messages (agent, session)
  WHERE delivered_at IS NULL;
