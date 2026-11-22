WITH named AS (
            SELECT c->>'entity_type' kind,(c->>'entity_id')::integer id,c->>'name' name
            FROM jsonb_array_elements(public.classifier_identity_candidates($1,$2,'')) c
            WHERE c->>'entity_type' IN ('player','person')
        ) SELECT COALESCE(jsonb_agg(jsonb_strip_nulls(jsonb_build_object(
            'entity_type',n.kind,'entity_id',n.id,'name',n.name,
            'team',CASE WHEN pci.source IN ('roster','override') THEN t.name END,
            'team_id',CASE WHEN pci.source IN ('roster','override') THEN t.id END,
            'source',pci.source,'observed_at',pci.source_updated_at::date::text))
            ORDER BY n.kind,n.id),'[]'::jsonb)
        FROM named n LEFT JOIN public.player_current_identity pci
            ON n.kind='player' AND pci.player_id=n.id AND pci.sport=$1
        LEFT JOIN public.teams t ON t.id=pci.team_id AND t.sport=pci.sport
