SELECT fallback_directory as directory, fallback_extension as extension
FROM beamline
WHERE name = ?;
