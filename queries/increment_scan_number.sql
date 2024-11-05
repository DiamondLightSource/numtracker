UPDATE beamline
SET scan_number = scan_number + 1
WHERE name = ?
RETURNING scan_number;
