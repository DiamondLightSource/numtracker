INSERT INTO scan_template (template)
VALUES (?)
ON CONFLICT (template) DO NOTHING
RETURNING id
