-- Rename visit column to directory
ALTER TABLE instrument
RENAME COLUMN visit TO directory;
