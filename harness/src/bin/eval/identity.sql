-- Read-only diagnostic. The caller owns a READ ONLY transaction.
WITH identity AS (
    SELECT to_jsonb(i) || jsonb_build_object('name', p.name, 'team_name', t.name) AS data
    FROM public.player_current_identity i
    JOIN public.players p ON p.id=i.player_id AND p.sport=i.sport
    LEFT JOIN public.teams t ON t.id=i.team_id AND t.sport=i.sport
    WHERE i.sport=$1 AND i.player_id=$2
), rosters AS (
    SELECT r.*, t.name AS team_name FROM public.team_rosters r
    LEFT JOIN public.teams t ON t.id=r.team_id AND t.sport=r.sport
    WHERE r.sport=$1 AND r.player_id=$2
), statistics AS (
    SELECT s.season,s.league_id,s.team_id,t.name AS team_name,s.updated_at,
           s.stats->'appearances' AS appearances,s.stats->'minutes_played' AS minutes
    FROM public.player_stats s LEFT JOIN public.teams t ON t.id=s.team_id AND t.sport=s.sport
    WHERE s.sport=$1 AND s.player_id=$2
    ORDER BY s.season DESC,s.updated_at DESC,s.league_id LIMIT 8
), applications AS (
    SELECT id,old_team_id,new_team_id,source_rumor_id,status,reason,created_at,applied_at,reverted_at
    FROM public.transfer_identity_applications WHERE sport=$1 AND player_id=$2
    ORDER BY created_at DESC,id DESC LIMIT 12
), rumors AS (
    SELECT DISTINCT ON (team_id) id,team_id,is_rumor,heat,stage,direction,generated_at,input_news_ids
    FROM public.transfer_rumors WHERE sport=$1 AND player_id=$2 AND subject_type='player'
    ORDER BY team_id,generated_at DESC,id DESC
)
SELECT jsonb_build_object(
    'captured_at',now(),
    'identity',(SELECT data FROM identity),
    'rosters',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY season DESC,last_seen DESC,team_id) FROM rosters r),'[]'::jsonb),
    'statistics',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY season DESC,updated_at DESC,league_id) FROM statistics s),'[]'::jsonb),
    'applications',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY created_at DESC,id DESC) FROM applications a),'[]'::jsonb),
    'active_overrides',COALESCE((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.applied_at DESC,o.id DESC)
        FROM public.player_current_identity_overrides o WHERE o.sport=$1 AND o.player_id=$2 AND o.reverted_at IS NULL),'[]'::jsonb),
    'latest_pair_verdicts',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY team_id) FROM rumors r),'[]'::jsonb),
    'identity_threshold',(SELECT to_jsonb(t) FROM public.transfer_identity_thresholds t WHERE sport=$1),
    'observations',jsonb_build_object(
        'stats_disagree_with_projection',EXISTS (
            SELECT 1 FROM statistics s,identity i WHERE s.season=(SELECT max(season) FROM statistics)
            AND s.team_id IS NOT NULL AND s.team_id IS DISTINCT FROM (i.data->>'team_id')::int),
        'active_roster_seasons',COALESCE((SELECT jsonb_agg(DISTINCT season) FROM rosters WHERE is_active),'[]'::jsonb),
        'note','Disagreement nominates verification; it does not authorize choosing the latest row or asserting a transfer. Heat and model verdicts are not completion evidence.'
    )
);
