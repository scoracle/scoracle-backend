BEGIN;
-- A known short name inside an unknown longer name is not a resolved identity.
CREATE FUNCTION public.classifier_name_span_matches(headline text, surface text, kind text)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE
    words text[];
    gaps text[];
    size integer := cardinality(string_to_array(surface,' '));
    n integer;
    previous text;
    following text;
    connectors text[] := ARRAY['a','an','the','and','or','but','to','from','for','at','with','against','vs','v','of','on','by','over','after','before','about','as','in','into','out','off','not','is','are','was','were','be','been','has','have','had','will','would','could','should','can','may','must','s','former','ex','new','old','current','next','first','last','star','player','players','coach','manager','boss','owner','captain','midfielder','forward','defender','goalkeeper','winger','striker','team','club','clubmate','legend','target','says','said','say','join','joins','joined','joining','leave','leaves','leaving','left','sign','signs','signed','signing','deny','denies','denied','reject','rejects','rejected','talks','move','moves','moving','transfer','transfers','news','update','updates','welcome','welcomes','welcomed','scores','scored','wins','win','won','beats','beat','faces','face','happy','backs','back','wants','want','wanted','linked','keen','return','returns','returned','confirms','confirmed','confident','urged','injury','injured','rules','ruled','interest','interested','under','around','through','without','during','why','how','what','when','monitor','monitors','monitoring','consider','considers','considering','report','reports','reported','reportedly','pursue','pursues','pursuing','cryptic','watch','question','one','two','three','four','five','six','seven','eight','nine','ten'];
BEGIN
    SELECT array_agg(m[1]),array_agg(m[2]) INTO words,gaps
    FROM regexp_matches(public.unaccent('public.unaccent'::regdictionary,headline),'([[:alnum:]]+)([^[:alnum:]]*)','g') m;
    IF size=0 OR words IS NULL OR cardinality(words)<size THEN RETURN false; END IF;
    FOR n IN 1..cardinality(words)-size+1 LOOP
        IF lower(array_to_string(words[n:n+size-1],' '))<>surface THEN CONTINUE; END IF;
        previous := lower(words[n-1]);
        following := lower(words[n+size]);
        -- shortcut: uncertain adjacent names abstain; extend connectors only with a regression case.
        IF size=1 AND n>1 AND gaps[n-1]!~ '[,;:!?()/–—-]' AND previous~'^[a-z]+$'
            AND (words[n-1]<>lower(words[n-1]) OR headline=lower(headline))
            AND NOT(previous=ANY(connectors)) THEN CONTINUE; END IF;
        IF following IS NOT NULL AND gaps[n+size-1]!~ '[,;:!?()/–—-]' THEN
            IF kind='team' AND following=ANY(ARRAY['women','womens','ladies','youth','academy','u18','u19','u21','u23','school','college']) THEN CONTINUE; END IF;
            IF kind IN ('player','person') AND size=1 AND following~'^[a-z]+$'
                AND NOT(following=ANY(connectors)) THEN CONTINUE; END IF;
        END IF;
        RETURN true;
    END LOOP;
    RETURN false;
END $$;

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
        AND public.classifier_name_span_matches(headline||' '||body,s.norm,s.entity_type)
    ), unique_names AS (
        SELECT s.* FROM matching_names s
        WHERE NOT EXISTS(SELECT 1 FROM public.entity_name_surfaces other
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
