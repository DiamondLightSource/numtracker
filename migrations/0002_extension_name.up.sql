-- Rename column to match renamed struct field
ALTER TABLE beamline
RENAME COLUMN fallback_extension TO tracker_file_extension;
