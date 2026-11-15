-- Messages to the agent are claimed by the status that hands them over and
-- delivered once the CLI acknowledges them, so a CLI killed in between
-- never loses one. A message sent again with its local_id is the same
-- message. The route, the run and the attempts serve the delivery to
-- agents Pinrail runs.
ALTER TABLE session_messages ADD COLUMN local_id TEXT;
ALTER TABLE session_messages ADD COLUMN route TEXT;          -- type, protocol, queue
ALTER TABLE session_messages ADD COLUMN state TEXT NOT NULL DEFAULT 'queued';
                                                             -- queued, typing, claimed, delivered, unconfirmed, typed
ALTER TABLE session_messages ADD COLUMN claimed_at TEXT;
ALTER TABLE session_messages ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session_messages ADD COLUMN run_id TEXT REFERENCES runs (id) ON DELETE SET NULL;
CREATE UNIQUE INDEX session_messages_local ON session_messages (agent, session, local_id)
  WHERE local_id IS NOT NULL;
UPDATE session_messages SET state = 'delivered' WHERE delivered_at IS NOT NULL;
