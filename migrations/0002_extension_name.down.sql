-- Revert back to fallback_extension name
ALTER TABLE beamline
RENAME COLUMN tracker_file_extension TO fallback_extension;
