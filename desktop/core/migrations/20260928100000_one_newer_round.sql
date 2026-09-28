-- A round has at most one newer round, so the rounds of a review form a
-- single line. SQLite allows any number of NULLs in a unique index.
DROP INDEX reviews_revises;
CREATE UNIQUE INDEX reviews_revises ON reviews(revises);
