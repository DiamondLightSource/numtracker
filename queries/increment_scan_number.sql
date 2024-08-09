UPDATE scan_number
SET last_number = previous + 1
FROM (
    SELECT beamline.id AS bl_id, scan_number.last_number AS previous
    FROM scan_number
        JOIN beamline ON scan_number.beamline = beamline.id
    WHERE beamline.name=?
)
WHERE beamline = bl_id
RETURNING last_number
