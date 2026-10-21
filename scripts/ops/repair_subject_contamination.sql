-- Preview by default. Apply only after deploying the sport-scoped writers.
-- psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -f scripts/ops/repair_subject_contamination.sql
-- Apply: add -v apply=true -v backup_file=/private/server/path/unique-backup.jsonl
-- The backup contains complete affected rows, never source prompts or credentials.
\set ON_ERROR_STOP on
\if :{?apply}
\else
  \set apply false
\endif

BEGIN;
SET LOCAL lock_timeout='3s';
SET LOCAL statement_timeout='30s';
LOCK TABLE public.insider_scores,public.transfer_rumors,public.sigil_synthesis,
    public.teams,public.players IN SHARE ROW EXCLUSIVE MODE;
LOCK TABLE public.entity_facts IN SHARE MODE;

-- These legacy validators accepted co-mentions as moves and have no reliable
-- subject-validation receipt. Withdraw the cohort rather than guess its meaning.
CREATE TEMP TABLE withdrawn_scores ON COMMIT DROP AS
SELECT * FROM public.insider_scores
WHERE prompt_version IN ('insider-source-v3','insider-source-v6-concise-body');
CREATE TEMP TABLE withdrawn_rumors ON COMMIT DROP AS
SELECT * FROM public.transfer_rumors
WHERE prompt_version IN ('insider-source-v3','insider-source-v6-concise-body');
CREATE TEMP TABLE withdrawn_sigils ON COMMIT DROP AS
SELECT g.* FROM public.sigil_synthesis g
WHERE EXISTS (
    SELECT 1 FROM withdrawn_scores s
    WHERE s.entity_type=g.entity_type AND s.entity_id=g.entity_id AND s.sport=g.sport
      AND s.read IS NOT NULL AND g.input_components #>> '{insider,body}'=s.read
);

-- Render each fact in the same display convention as the original writer.
CREATE TEMP TABLE sourced_cells ON COMMIT DROP AS
SELECT id,entity_type,entity_id,sport,created_at,
    CASE fact_type WHEN 'weight_kg' THEN 'weight' WHEN 'height_cm' THEN 'height' ELSE fact_type END field,
    CASE
      WHEN fact_type='weight_kg' AND value_text ~ '^[0-9]+([.][0-9]+)?$' THEN
        CASE WHEN sport IN ('NBA','NFL') THEN round(value_text::numeric*2.20462)::text||' lbs'
             ELSE round(value_text::numeric)::text||' kg' END
      WHEN fact_type='height_cm' AND value_text ~ '^[0-9]+([.][0-9]+)?$' THEN
        CASE WHEN sport IN ('NBA','NFL') THEN
          (round(value_text::numeric/2.54)::int/12)::text||''''||
          (round(value_text::numeric/2.54)::int%12)::text||'"'
        ELSE round(value_text::numeric)::text||' cm' END
      WHEN fact_type NOT IN ('weight_kg','height_cm') THEN value_text
    END value
FROM public.entity_facts
WHERE state='active' AND (
    (entity_type='team' AND fact_type IN ('logo_url','venue_name')) OR
    (entity_type='player' AND fact_type IN ('date_of_birth','weight_kg','height_cm','photo_url'))
);
CREATE TEMP TABLE repair_cells ON COMMIT DROP AS
WITH current_cells AS (
    SELECT 'team'::text entity_type,t.id,t.sport,v.field,v.value
    FROM public.teams t CROSS JOIN LATERAL(VALUES('logo_url',t.logo_url),('venue_name',t.venue_name)) v(field,value)
    UNION ALL
    SELECT 'player',p.id,p.sport,v.field,v.value FROM public.players p
    CROSS JOIN LATERAL(VALUES('date_of_birth',p.date_of_birth::text),('weight',p.weight),('height',p.height),('photo_url',p.photo_url)) v(field,value)
)
SELECT c.*,(
    SELECT own.value FROM sourced_cells own
    WHERE own.entity_type=c.entity_type AND own.entity_id=c.id AND own.sport=c.sport AND own.field=c.field
    ORDER BY own.created_at DESC,own.id DESC LIMIT 1
) replacement
FROM current_cells c
WHERE c.value IS NOT NULL AND EXISTS (
    SELECT 1 FROM sourced_cells other WHERE other.entity_type=c.entity_type AND other.entity_id=c.id
      AND other.sport<>c.sport AND other.field=c.field AND other.value=c.value
) AND NOT EXISTS (
    SELECT 1 FROM sourced_cells own WHERE own.entity_type=c.entity_type AND own.entity_id=c.id
      AND own.sport=c.sport AND own.field=c.field AND own.value=c.value
);

SELECT 'insider_scores' kind,count(*) rows FROM withdrawn_scores
UNION ALL SELECT 'transfer_rumors',count(*) FROM withdrawn_rumors
UNION ALL SELECT 'sigil_synthesis',count(*) FROM withdrawn_sigils;
SELECT entity_type,sport,field,count(*) fields,count(*) FILTER(WHERE replacement IS NULL) cleared
FROM repair_cells GROUP BY entity_type,sport,field ORDER BY entity_type,sport,field;

\if :apply
  \if :{?backup_file}
  \else
    \echo 'Apply requires a unique backup_file on the database operator host.'
    \quit 1
  \endif
  DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM public.harvester_insider_wraps w JOIN withdrawn_scores s ON s.id=w.score_id)
       OR EXISTS(SELECT 1 FROM public.harvester_insider_identity_reviews v JOIN withdrawn_rumors r ON r.id=v.transfer_rumor_id) THEN
      RAISE EXCEPTION 'Unexpected legacy dependents; stop and include them in the recovery plan';
    END IF;
  END $$;
  \pset format unaligned
  \pset tuples_only on
  \o :backup_file
  SELECT jsonb_build_object('table','insider_scores','row',to_jsonb(s)) FROM withdrawn_scores s
  UNION ALL SELECT jsonb_build_object('table','transfer_rumors','row',to_jsonb(r)) FROM withdrawn_rumors r
  UNION ALL SELECT jsonb_build_object('table','sigil_synthesis','row',to_jsonb(g)) FROM withdrawn_sigils g
  UNION ALL SELECT jsonb_build_object('table','teams','row',to_jsonb(t)) FROM public.teams t
    WHERE EXISTS(SELECT 1 FROM repair_cells c WHERE c.entity_type='team' AND c.id=t.id AND c.sport=t.sport)
  UNION ALL SELECT jsonb_build_object('table','players','row',to_jsonb(p)) FROM public.players p
    WHERE EXISTS(SELECT 1 FROM repair_cells c WHERE c.entity_type='player' AND c.id=p.id AND c.sport=p.sport)
  UNION ALL SELECT jsonb_build_object('repair',to_jsonb(c)) FROM repair_cells c;
  \o
  \pset tuples_only off
  \pset format aligned

  DELETE FROM public.sigil_synthesis WHERE id IN(SELECT id FROM withdrawn_sigils);
  DELETE FROM public.transfer_rumors WHERE id IN(SELECT id FROM withdrawn_rumors);
  DELETE FROM public.insider_scores WHERE id IN(SELECT id FROM withdrawn_scores);
  UPDATE public.teams t SET
    logo_url=CASE WHEN c.cells ? 'logo_url' THEN c.cells->>'logo_url' ELSE t.logo_url END,
    venue_name=CASE WHEN c.cells ? 'venue_name' THEN c.cells->>'venue_name' ELSE t.venue_name END,
    updated_at=now()
  FROM (SELECT id,sport,jsonb_object_agg(field,replacement) cells FROM repair_cells WHERE entity_type='team' GROUP BY id,sport) c
  WHERE t.id=c.id AND t.sport=c.sport;
  UPDATE public.players p SET
    date_of_birth=CASE WHEN c.cells ? 'date_of_birth' THEN (c.cells->>'date_of_birth')::date ELSE p.date_of_birth END,
    weight=CASE WHEN c.cells ? 'weight' THEN c.cells->>'weight' ELSE p.weight END,
    height=CASE WHEN c.cells ? 'height' THEN c.cells->>'height' ELSE p.height END,
    photo_url=CASE WHEN c.cells ? 'photo_url' THEN c.cells->>'photo_url' ELSE p.photo_url END,
    updated_at=now()
  FROM (SELECT id,sport,jsonb_object_agg(field,replacement) cells FROM repair_cells WHERE entity_type='player' GROUP BY id,sport) c
  WHERE p.id=c.id AND p.sport=c.sport;
  COMMIT;
\else
  ROLLBACK;
\endif
