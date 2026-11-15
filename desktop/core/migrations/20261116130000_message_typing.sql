-- Why a message to be typed into an agent's terminal waits (working,
-- needs_input, used_since_idle, not_foreground), and when it was typed,
-- from which an unconfirmed message is told.
ALTER TABLE session_messages ADD COLUMN held TEXT;
ALTER TABLE session_messages ADD COLUMN typed_at TEXT;
