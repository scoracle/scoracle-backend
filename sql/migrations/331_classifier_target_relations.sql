BEGIN;
SET LOCAL lock_timeout='2s';
SET LOCAL statement_timeout='15s';

-- Bind the retained headline to the exact native request in either envelope.
ALTER TABLE public.classifier_spectra DROP CONSTRAINT classifier_spectra_complete_receipt;
ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
    CHECK (COALESCE(gate->>'coverage'='complete'
        AND CASE WHEN receipt->>'request_version' IN ('classifier-brief-v2','classifier-brief-v3') THEN
            CASE WHEN jsonb_typeof(receipt#>'{request,state}')='string'
                THEN (regexp_match(receipt#>>'{request,state}', '(?n)^Headline: (.*)$'))[1]::jsonb#>>'{}'
            END
        WHEN receipt->>'request_version'='classifier-assignment-v1' THEN
            CASE WHEN jsonb_typeof(receipt#>'{request,state}')='string'
                THEN (receipt#>>'{request,state}')::jsonb#>>'{new_content,headline}'
            END
        ELSE COALESCE(receipt#>>'{request,state,article,headline}',
                      receipt#>>'{request,state,article,body}',receipt#>>'{request,state,body}')
        END = clean_text
        AND receipt#>>'{response,model}'=model
        AND request_hash<>'',false)) NOT VALID;


CREATE OR REPLACE FUNCTION public.classifier_article_ready() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,x.clean_text,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version IN ('classification-criteria-v9-headlines','classification-criteria-v10-availability','classification-criteria-v11-targeted','classification-criteria-v12-newsworthy','classification-criteria-v13-relations') THEN
        IF NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)
            OR measured.source->>'evidence_scope' IS DISTINCT FROM 'headline'
            OR length(btrim(measured.clean_text))=0
            OR measured.clean_text IS DISTINCT FROM measured.source->>'title' THEN
            RAISE EXCEPTION 'Headline delivery requires an admitted character and retained headline';
        END IF;
    ELSIF measured.criteria_version IN ('classification-criteria-v7-headline','classification-criteria-v8-admission') AND
        (length(btrim(COALESCE(measured.source->>'body','')))=0
         OR NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)) THEN
        RAISE EXCEPTION 'Historical delivery requires an admitted character and retained article';
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.classifier_name_span_matches(headline text, surface text, kind text)
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
            -- A suffix is part of a person's full name, even for multiword names.
            IF kind IN ('player','person') AND following=ANY(ARRAY['jr','sr','junior','senior','ii','iii','iv']) THEN CONTINUE; END IF;
            IF kind='team' AND following=ANY(ARRAY['women','womens','ladies','youth','academy','u18','u19','u21','u23','school','college']) THEN CONTINUE; END IF;
            IF kind IN ('player','person') AND size=1 AND following~'^[a-z]+$'
                AND NOT(following=ANY(connectors)) THEN CONTINUE; END IF;
        END IF;
        RETURN true;
    END LOOP;
    RETURN false;
END $$;


INSERT INTO public.schema_migrations(version) VALUES('331_classifier_target_relations') ON CONFLICT DO NOTHING;
COMMIT;
