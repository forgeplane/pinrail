-- The boot of the computer the core last saw a host in, so that a host
-- found in a new boot tells the core the computer restarted, and when the
-- person last dismissed the agents a restart stopped.
ALTER TABLE hosts ADD COLUMN boot_id TEXT;
ALTER TABLE hosts ADD COLUMN restart_seen_at TEXT;
