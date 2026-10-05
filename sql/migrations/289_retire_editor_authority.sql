-- Retire legacy packet scheduling and Editor-based canonical identity eligibility.
-- Historical Editor/packet/storyline rows remain readable; no data is deleted.
DROP TRIGGER IF EXISTS enqueue_voices_on_packet ON public.packets;

CREATE OR REPLACE FUNCTION public.settled_transfer_identity_evidence(
    p_sport text, p_player_id integer, p_team_id integer, p_news_ids bigint[]
) RETURNS TABLE(eligible boolean, stats_season integer, article_ids bigint[], source_names text[])
LANGUAGE sql STABLE AS $$
    SELECT false, NULL::integer, ARRAY[]::bigint[], ARRAY[]::text[];
$$;
COMMENT ON FUNCTION public.settled_transfer_identity_evidence(text, integer, integer, bigint[])
IS 'Retired Editor-based identity authority. Always unavailable; canonical identity requires supported retained evidence.';

INSERT INTO public.schema_migrations(version)
VALUES ('289_retire_editor_authority') ON CONFLICT (version) DO NOTHING;
