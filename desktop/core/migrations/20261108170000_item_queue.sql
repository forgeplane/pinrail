-- The person's queue: the items agents take first, in this order. A move
-- writes one row, at a position between its new neighbours.
CREATE TABLE work_item_queue (
  item_id  TEXT PRIMARY KEY REFERENCES work_items(id) ON DELETE CASCADE,
  position REAL NOT NULL
);
CREATE INDEX work_item_queue_position ON work_item_queue(position);
