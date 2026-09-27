-- Each source/subject pair is an independent, resumable Insider obligation.
-- The source assignment can close only after every pair has a terminal verdict.
BEGIN;
CREATE TABLE IF NOT EXISTS public.harvester_insider_pairs (
    classification_id bigint NOT NULL REFERENCES public.harvester_classifications(id) ON DELETE CASCADE,
    subject_type text NOT NULL CHECK (subject_type IN ('player','person')),
    subject_id integer NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','rumor','cleared')),
    product_ref jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (classification_id,subject_type,subject_id)
);
CREATE INDEX IF NOT EXISTS harvester_insider_pairs_pending_idx
    ON public.harvester_insider_pairs(classification_id,subject_type,subject_id)
    WHERE status='pending';
COMMENT ON TABLE public.harvester_insider_pairs IS
    'Claim-fenced Insider verdict per resolved Harvester source/subject pair; no pair is dropped by a prompt cap.';
INSERT INTO public.schema_migrations(version)
VALUES ('274_harvester_insider_pairs') ON CONFLICT DO NOTHING;
COMMIT;
