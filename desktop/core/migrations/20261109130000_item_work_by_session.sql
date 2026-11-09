-- What a session did to work items, and the comments it wrote, found by
-- the session rather than by reading every item's events and comments.
CREATE INDEX work_item_events_session ON work_item_events(actor, session, id);
CREATE INDEX work_item_comments_session ON work_item_comments(author, session, created_at, id);
