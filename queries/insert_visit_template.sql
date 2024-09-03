INSERT INTO visit_template (template)
VALUES (?)
ON CONFLICT (template) DO NOTHING
RETURNING id
