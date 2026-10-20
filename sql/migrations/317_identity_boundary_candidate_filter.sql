BEGIN;
-- Check expensive boundaries only after materializing literal name matches.
CREATE OR REPLACE FUNCTION public.classifier_identity_candidates(league text, headline text, body text)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    WITH input AS MATERIALIZED (
        SELECT ' '||public.nrm(headline||' '||body)||' ' AS text
    ), words AS MATERIALIZED (
        SELECT DISTINCT unnest(string_to_array(input.text,' ')) AS word FROM input
    ), matching_names AS MATERIALIZED (
        SELECT s.entity_type,s.entity_id,s.norm FROM public.entity_name_surfaces s
        JOIN words ON words.word=split_part(s.norm,' ',1) CROSS JOIN input
        WHERE s.sport=league AND s.surface_kind IN ('name','alias') AND s.norm<>''
        AND strpos(input.text,' '||s.norm||' ')>0
    ), unique_names AS (
        SELECT s.* FROM matching_names s
        WHERE public.classifier_name_span_matches(headline||' '||body,s.norm,s.entity_type)
        AND NOT EXISTS(SELECT 1 FROM public.entity_name_surfaces other
            WHERE other.sport=league AND other.norm=s.norm
            AND (other.entity_type,other.entity_id)<>(s.entity_type,s.entity_id))
    ), candidates AS (
        SELECT DISTINCT n.entity_type,n.entity_id,COALESCE(t.name,p.name,pp.full_name) AS name
        FROM unique_names n
        LEFT JOIN public.teams t ON n.entity_type='team' AND t.id=n.entity_id AND t.sport=league
        LEFT JOIN public.players p ON n.entity_type='player' AND p.id=n.entity_id AND p.sport=league
        LEFT JOIN public.persons pp ON n.entity_type='person' AND pp.id=n.entity_id AND pp.sport=league
    ) SELECT COALESCE(jsonb_agg(jsonb_build_object('entity_type',entity_type,'entity_id',entity_id,
        'sport',league,'name',name,'identity_method','unique_name_surface_v2')
        ORDER BY entity_type,entity_id) FILTER(WHERE name IS NOT NULL),'[]'::jsonb) FROM candidates
$$;


COMMIT;
