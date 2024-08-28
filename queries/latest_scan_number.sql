SELECT last_number
FROM scan_number
JOIN beamline
ON beamline.id = scan_number.beamline
WHERE beamline.name = ?;
