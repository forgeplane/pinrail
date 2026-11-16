-- The full-text index of the work items: each item's title, body and
-- comments, with word forms matched by the porter stemmer. It holds
-- nothing of its own: the triggers keep it in line with the items and
-- their comments, and this migration fills it.
--
-- An item's row in the index is numbered by work_item_search_rows, whose
-- integer key stays the same through a VACUUM, where an implicit rowid
-- may not.
CREATE TABLE work_item_search_rows (
  row     INTEGER PRIMARY KEY,
  item_id TEXT NOT NULL UNIQUE
);

CREATE VIRTUAL TABLE work_item_search USING fts5(
  title,
  body,
  comments,
  tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER work_item_search_insert AFTER INSERT ON work_items BEGIN
  INSERT INTO work_item_search_rows (item_id) VALUES (new.id);
  INSERT INTO work_item_search (rowid, title, body, comments)
    VALUES ((SELECT row FROM work_item_search_rows WHERE item_id = new.id), new.title, new.body, '');
END;

CREATE TRIGGER work_item_search_update AFTER UPDATE OF title, body ON work_items BEGIN
  UPDATE work_item_search SET title = new.title, body = new.body
    WHERE rowid = (SELECT row FROM work_item_search_rows WHERE item_id = new.id);
END;

CREATE TRIGGER work_item_search_delete AFTER DELETE ON work_items BEGIN
  DELETE FROM work_item_search
    WHERE rowid = (SELECT row FROM work_item_search_rows WHERE item_id = old.id);
  DELETE FROM work_item_search_rows WHERE item_id = old.id;
END;

-- an item's comments, oldest first, one after another
CREATE TRIGGER work_item_search_comment_insert AFTER INSERT ON work_item_comments BEGIN
  UPDATE work_item_search SET comments = (
      SELECT IFNULL(group_concat(body, char(10)), '')
        FROM (SELECT body FROM work_item_comments WHERE item_id = new.item_id ORDER BY created_at, id))
    WHERE rowid = (SELECT row FROM work_item_search_rows WHERE item_id = new.item_id);
END;

CREATE TRIGGER work_item_search_comment_update AFTER UPDATE OF body ON work_item_comments BEGIN
  UPDATE work_item_search SET comments = (
      SELECT IFNULL(group_concat(body, char(10)), '')
        FROM (SELECT body FROM work_item_comments WHERE item_id = new.item_id ORDER BY created_at, id))
    WHERE rowid = (SELECT row FROM work_item_search_rows WHERE item_id = new.item_id);
END;

CREATE TRIGGER work_item_search_comment_delete AFTER DELETE ON work_item_comments BEGIN
  UPDATE work_item_search SET comments = (
      SELECT IFNULL(group_concat(body, char(10)), '')
        FROM (SELECT body FROM work_item_comments WHERE item_id = old.item_id ORDER BY created_at, id))
    WHERE rowid = (SELECT row FROM work_item_search_rows WHERE item_id = old.item_id);
END;

INSERT INTO work_item_search_rows (item_id) SELECT id FROM work_items ORDER BY created_at, id;

INSERT INTO work_item_search (rowid, title, body, comments)
  SELECT r.row, i.title, i.body, (
      SELECT IFNULL(group_concat(body, char(10)), '')
        FROM (SELECT body FROM work_item_comments WHERE item_id = i.id ORDER BY created_at, id))
    FROM work_item_search_rows r JOIN work_items i ON i.id = r.item_id;
