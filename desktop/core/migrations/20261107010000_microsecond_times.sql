-- Every time to the microsecond, always with six digits, as the core now
-- writes them, so that times compare as text in the order they happened:
-- `2026-11-07T10:00:00Z` becomes `2026-11-07T10:00:00.000000Z`, and a time
-- written to the millisecond gets three more digits.
UPDATE blobs SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE events SET at = CASE length(at)
    WHEN 20 THEN substr(at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(at, 1, 23) || '000Z'
    ELSE at END
  WHERE at IS NOT NULL AND length(at) <> 27;
UPDATE outcomes SET at = CASE length(at)
    WHEN 20 THEN substr(at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(at, 1, 23) || '000Z'
    ELSE at END
  WHERE at IS NOT NULL AND length(at) <> 27;
UPDATE plugin_bundles SET stored_at = CASE length(stored_at)
    WHEN 20 THEN substr(stored_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(stored_at, 1, 23) || '000Z'
    ELSE stored_at END
  WHERE stored_at IS NOT NULL AND length(stored_at) <> 27;
UPDATE plugin_installs SET installed_at = CASE length(installed_at)
    WHEN 20 THEN substr(installed_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(installed_at, 1, 23) || '000Z'
    ELSE installed_at END
  WHERE installed_at IS NOT NULL AND length(installed_at) <> 27;
UPDATE plugin_installs SET updated_at = CASE length(updated_at)
    WHEN 20 THEN substr(updated_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(updated_at, 1, 23) || '000Z'
    ELSE updated_at END
  WHERE updated_at IS NOT NULL AND length(updated_at) <> 27;
UPDATE plugin_skill_grants SET granted_at = CASE length(granted_at)
    WHEN 20 THEN substr(granted_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(granted_at, 1, 23) || '000Z'
    ELSE granted_at END
  WHERE granted_at IS NOT NULL AND length(granted_at) <> 27;
UPDATE projects SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE repos SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE reviews SET expires_at = CASE length(expires_at)
    WHEN 20 THEN substr(expires_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(expires_at, 1, 23) || '000Z'
    ELSE expires_at END
  WHERE expires_at IS NOT NULL AND length(expires_at) <> 27;
UPDATE reviews SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE reviews SET starred_at = CASE length(starred_at)
    WHEN 20 THEN substr(starred_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(starred_at, 1, 23) || '000Z'
    ELSE starred_at END
  WHERE starred_at IS NOT NULL AND length(starred_at) <> 27;
UPDATE schema_migrations SET inserted_at = CASE length(inserted_at)
    WHEN 20 THEN substr(inserted_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(inserted_at, 1, 23) || '000Z'
    ELSE inserted_at END
  WHERE inserted_at IS NOT NULL AND length(inserted_at) <> 27;
UPDATE session_messages SET sent_at = CASE length(sent_at)
    WHEN 20 THEN substr(sent_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(sent_at, 1, 23) || '000Z'
    ELSE sent_at END
  WHERE sent_at IS NOT NULL AND length(sent_at) <> 27;
UPDATE session_messages SET delivered_at = CASE length(delivered_at)
    WHEN 20 THEN substr(delivered_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(delivered_at, 1, 23) || '000Z'
    ELSE delivered_at END
  WHERE delivered_at IS NOT NULL AND length(delivered_at) <> 27;
UPDATE session_statuses SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE sessions SET first_at = CASE length(first_at)
    WHEN 20 THEN substr(first_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(first_at, 1, 23) || '000Z'
    ELSE first_at END
  WHERE first_at IS NOT NULL AND length(first_at) <> 27;
UPDATE sessions SET last_at = CASE length(last_at)
    WHEN 20 THEN substr(last_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(last_at, 1, 23) || '000Z'
    ELSE last_at END
  WHERE last_at IS NOT NULL AND length(last_at) <> 27;
UPDATE sessions SET archived_at = CASE length(archived_at)
    WHEN 20 THEN substr(archived_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(archived_at, 1, 23) || '000Z'
    ELSE archived_at END
  WHERE archived_at IS NOT NULL AND length(archived_at) <> 27;
UPDATE work_item_comments SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE work_item_comments SET edited_at = CASE length(edited_at)
    WHEN 20 THEN substr(edited_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(edited_at, 1, 23) || '000Z'
    ELSE edited_at END
  WHERE edited_at IS NOT NULL AND length(edited_at) <> 27;
UPDATE work_item_events SET at = CASE length(at)
    WHEN 20 THEN substr(at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(at, 1, 23) || '000Z'
    ELSE at END
  WHERE at IS NOT NULL AND length(at) <> 27;
UPDATE work_items SET created_at = CASE length(created_at)
    WHEN 20 THEN substr(created_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(created_at, 1, 23) || '000Z'
    ELSE created_at END
  WHERE created_at IS NOT NULL AND length(created_at) <> 27;
UPDATE work_items SET updated_at = CASE length(updated_at)
    WHEN 20 THEN substr(updated_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(updated_at, 1, 23) || '000Z'
    ELSE updated_at END
  WHERE updated_at IS NOT NULL AND length(updated_at) <> 27;
UPDATE work_items SET closed_at = CASE length(closed_at)
    WHEN 20 THEN substr(closed_at, 1, 19) || '.000000Z'
    WHEN 24 THEN substr(closed_at, 1, 23) || '000Z'
    ELSE closed_at END
  WHERE closed_at IS NOT NULL AND length(closed_at) <> 27;

-- The thread orders its entries by these times, so nothing needs to say
-- which status a message or a comment came after.
ALTER TABLE session_messages DROP COLUMN after_status;
ALTER TABLE work_item_comments DROP COLUMN after_status;
