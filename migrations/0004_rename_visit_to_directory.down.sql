-- Revert directory column back to visit
ALTER TABLE instrument
RENAME COLUMN directory TO visit;
