INSERT INTO detector_template (template)
VALUES (?)
ON CONFLICT (template) DO NOTHING
RETURNING id
