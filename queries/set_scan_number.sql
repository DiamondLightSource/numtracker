UPDATE scan_number
SET last_number = ?
FROM (
    SELECT beamline.id AS bl_id
    FROM scan_number
        JOIN beamline ON scan_number.beamline = beamline.id
    WHERE beamline.name = ?
)
WHERE beamline = bl_id;
