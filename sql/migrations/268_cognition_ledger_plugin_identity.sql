-- Add the owning plugin identity without changing existing diagnostic readers.
-- Nullable is intentional for rolling compatibility with older binaries and any
-- historical rows whose stage does not map to the current statically linked fleet.
BEGIN;

ALTER TABLE public.cognition_ledger
    ADD COLUMN plugin_id text;

UPDATE public.cognition_ledger
SET plugin_id = CASE stage
    WHEN 'editor' THEN 'scoracle.internal.editor'
    WHEN 'graph' THEN 'scoracle.internal.graph'
    WHEN 'rating' THEN 'scoracle.character.rating'
    WHEN 'momentum' THEN 'scoracle.character.momentum'
    WHEN 'transfers' THEN 'scoracle.character.transfers'
    WHEN 'narratives' THEN 'scoracle.character.narrative'
    WHEN 'vibe' THEN 'scoracle.character.vibe'
    WHEN 'sigil' THEN 'scoracle.character.sigil'
    ELSE plugin_id
END
WHERE plugin_id IS NULL;

COMMENT ON COLUMN public.cognition_ledger.plugin_id IS
    'Stable ID of the plugin that owned this diagnostic generation; nullable for rolling compatibility and historical unknown stages.';

INSERT INTO public.schema_migrations(version)
VALUES ('268_cognition_ledger_plugin_identity')
ON CONFLICT DO NOTHING;

COMMIT;
