-- When a review ended, kept on the review beside its outcome, so that the
-- reviews still waiting are read through a small index rather than by
-- probing the outcomes of every review ever kept. An outcome is written
-- once and goes only with its review, so the trigger keeps the two in step.
ALTER TABLE reviews ADD COLUMN ended_at TEXT;
UPDATE reviews SET ended_at = (SELECT o.at FROM outcomes o WHERE o.review_id = reviews.id);
CREATE TRIGGER reviews_ended AFTER INSERT ON outcomes BEGIN
  UPDATE reviews SET ended_at = NEW.at WHERE id = NEW.review_id;
END;
CREATE INDEX reviews_open ON reviews(id) WHERE ended_at IS NULL;
