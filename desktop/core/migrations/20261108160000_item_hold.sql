-- An item on hold keeps the status 'open' and has the time the person put
-- it on hold here; the core reads the two as the status 'on_hold'. The
-- status column's CHECK cannot widen without rebuilding the table.
ALTER TABLE work_items ADD COLUMN held_at TEXT;
