-- A review or a question belongs to any number of work items. Every round
-- of a review has the same set; deleting an item drops only its links.
CREATE TABLE review_items (
  review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
  item_id   TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
  PRIMARY KEY (review_id, item_id)
);
CREATE INDEX review_items_item ON review_items(item_id);

INSERT INTO review_items (review_id, item_id)
  SELECT id, item_id FROM reviews WHERE item_id IS NOT NULL;
DROP INDEX reviews_item;
ALTER TABLE reviews DROP COLUMN item_id;
