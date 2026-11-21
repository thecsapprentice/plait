-- The rebrand (R3, #84): "plait" joins the starting reserved words. Defaults
-- are seeded only into an empty table, so a database seeded before the
-- rename gets it here. "quill" stays reserved, as it already was.
INSERT INTO reserved_handles (pattern, kind, source)
VALUES ('plait', 'word', 'default')
ON CONFLICT DO NOTHING;
