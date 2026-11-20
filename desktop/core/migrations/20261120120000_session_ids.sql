-- Every session has an id of Pinrail's own, `ses_` and a ULID, from the
-- moment Pinrail knows of it. The agent's own id for the session becomes a
-- field of it, `agent_session`, which is empty until the agent reports it.
-- The tables keep their shape: `sessions.id` and every `session` column
-- that names a session hold Pinrail's id from now on.
--
-- The parent keys change before the rows that refer to them, so the
-- foreign keys are checked when the migration commits.
PRAGMA defer_foreign_keys = ON;

-- The agent's own id for the session, such as Claude Code's UUID. Unique
-- per agent where it is known.
ALTER TABLE sessions ADD COLUMN agent_session TEXT;

-- Every session that something names by the agent's id: the reviews, the
-- work items and their events and comments, and the outcomes.
CREATE TEMP TABLE named_sessions (agent TEXT NOT NULL, id TEXT NOT NULL, at TEXT NOT NULL);
INSERT INTO named_sessions
  SELECT requested_by, session, created_at FROM reviews
  WHERE requested_by IS NOT NULL AND session IS NOT NULL;
INSERT INTO named_sessions
  SELECT claim_agent, claim_session, updated_at FROM work_items
  WHERE claim_agent IS NOT NULL AND claim_session IS NOT NULL;
INSERT INTO named_sessions
  SELECT created_by, created_session, created_at FROM work_items
  WHERE created_by <> 'person' AND created_session IS NOT NULL;
INSERT INTO named_sessions
  SELECT actor, session, at FROM work_item_events
  WHERE actor <> 'person' AND session IS NOT NULL;
INSERT INTO named_sessions
  SELECT author, session, created_at FROM work_item_comments
  WHERE author <> 'person' AND session IS NOT NULL;
INSERT INTO named_sessions
  SELECT by, session, at FROM outcomes
  WHERE by IS NOT NULL AND session IS NOT NULL AND via IS NOT 'app';

-- A session named without a row of its own, such as one the person
-- deleted, gets a row, hidden, so that the agent's id still leads to
-- Pinrail's.
INSERT INTO sessions (agent, id, first_at, last_at, hidden)
  SELECT n.agent, n.id, MIN(n.at), MAX(n.at), 1 FROM named_sessions n
  WHERE NOT EXISTS (SELECT 1 FROM sessions s WHERE s.agent = n.agent AND s.id = n.id)
  GROUP BY n.agent, n.id;
DROP TABLE named_sessions;

-- Pinrail's id for each session, by the agent and the agent's id.
CREATE TEMP TABLE session_ids (
  agent TEXT NOT NULL,
  old   TEXT NOT NULL,
  new   TEXT NOT NULL,
  PRIMARY KEY (agent, old)
);
CREATE INDEX temp.session_ids_old ON session_ids (old);
INSERT INTO session_ids SELECT agent, id, new_session_id() FROM sessions;

UPDATE sessions SET agent_session = id,
  id = (SELECT m.new FROM session_ids m WHERE m.agent = sessions.agent AND m.old = sessions.id);
CREATE UNIQUE INDEX sessions_id ON sessions (id);
CREATE UNIQUE INDEX sessions_agent_session ON sessions (agent, agent_session)
  WHERE agent_session IS NOT NULL;

UPDATE session_statuses SET session = (SELECT m.new FROM session_ids m
  WHERE m.agent = session_statuses.agent AND m.old = session_statuses.session);
UPDATE session_messages SET session = (SELECT m.new FROM session_ids m
  WHERE m.agent = session_messages.agent AND m.old = session_messages.session);
UPDATE session_presence SET session = (SELECT m.new FROM session_ids m
  WHERE m.agent = session_presence.agent AND m.old = session_presence.session);
UPDATE session_turns SET session = (SELECT m.new FROM session_ids m
  WHERE m.agent = session_turns.agent AND m.old = session_turns.session);
UPDATE run_sessions SET session = (SELECT m.new FROM session_ids m
  WHERE m.agent = run_sessions.agent AND m.old = run_sessions.session);

-- A reference without its agent is matched by the agent's id alone.
UPDATE reviews SET session = COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = reviews.requested_by AND m.old = reviews.session),
    (SELECT m.new FROM session_ids m WHERE reviews.requested_by IS NULL AND m.old = reviews.session
     ORDER BY m.new LIMIT 1))
  WHERE session IS NOT NULL;
UPDATE outcomes SET session = COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = outcomes.by AND m.old = outcomes.session),
    (SELECT m.new FROM session_ids m WHERE m.old = outcomes.session ORDER BY m.new LIMIT 1))
  WHERE session IS NOT NULL;
UPDATE work_items SET claim_session = (SELECT m.new FROM session_ids m
    WHERE m.agent = work_items.claim_agent AND m.old = work_items.claim_session)
  WHERE claim_session IS NOT NULL;
UPDATE work_items SET created_session = (SELECT m.new FROM session_ids m
    WHERE m.agent = work_items.created_by AND m.old = work_items.created_session)
  WHERE created_session IS NOT NULL;
UPDATE work_item_events SET session = (SELECT m.new FROM session_ids m
    WHERE m.agent = work_item_events.actor AND m.old = work_item_events.session)
  WHERE session IS NOT NULL;
UPDATE work_item_comments SET session = (SELECT m.new FROM session_ids m
    WHERE m.agent = work_item_comments.author AND m.old = work_item_comments.session)
  WHERE session IS NOT NULL;

-- The sessions the items' events name in what they carry: the claim an
-- item was taken from, and the session whose claims were released.
UPDATE work_item_events SET attrs = json_set(attrs, '$.from.session', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.from.agent')
       AND m.old = json_extract(attrs, '$.from.session')),
    json_extract(attrs, '$.from.session')))
  WHERE json_valid(attrs) AND json_type(attrs, '$.from.session') = 'text';
UPDATE work_item_events SET attrs = json_set(attrs, '$.session', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.agent')
       AND m.old = json_extract(attrs, '$.session')),
    json_extract(attrs, '$.session')))
  WHERE json_valid(attrs) AND json_type(attrs, '$.session') = 'text';

-- The sessions the stored events name: a session's own events by `agent`
-- and `id`, an item's change by `agent` and `session`, and a run's by its
-- session and the one it left.
UPDATE events SET attrs = json_set(attrs, '$.id', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.agent')
       AND m.old = json_extract(attrs, '$.id')),
    json_extract(attrs, '$.id')))
  WHERE kind LIKE 'session\_%' ESCAPE '\' AND json_valid(attrs)
    AND json_type(attrs, '$.id') = 'text';
UPDATE events SET attrs = json_set(attrs, '$.session', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.agent')
       AND m.old = json_extract(attrs, '$.session')),
    json_extract(attrs, '$.session')))
  WHERE kind LIKE 'item\_%' ESCAPE '\' AND json_valid(attrs)
    AND json_type(attrs, '$.session') = 'text';
UPDATE events SET attrs = json_set(attrs, '$.session.id', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.session.agent')
       AND m.old = json_extract(attrs, '$.session.id')),
    json_extract(attrs, '$.session.id')))
  WHERE kind = 'run_bound' AND json_valid(attrs) AND json_type(attrs, '$.session.id') = 'text';
UPDATE events SET attrs = json_set(attrs, '$.previous.id', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.previous.agent')
       AND m.old = json_extract(attrs, '$.previous.id')),
    json_extract(attrs, '$.previous.id')))
  WHERE kind = 'run_bound' AND json_valid(attrs) AND json_type(attrs, '$.previous.id') = 'text';
UPDATE events SET attrs = json_set(attrs, '$.run.session.id', COALESCE(
    (SELECT m.new FROM session_ids m WHERE m.agent = json_extract(attrs, '$.run.session.agent')
       AND m.old = json_extract(attrs, '$.run.session.id')),
    json_extract(attrs, '$.run.session.id')))
  WHERE kind IN ('run_started', 'run_ended') AND json_valid(attrs)
    AND json_type(attrs, '$.run.session.id') = 'text';

DROP TABLE session_ids;
