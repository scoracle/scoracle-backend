-- 200_one_rail_storylines.sql
--
-- WHAT: `storylines`, `storyline_articles`, `storyline_entities` — the greenfield rail's
-- story-assembly substrate, exactly per PLAN-one-rail.md §1b.
--
-- WHY: the packet (§1c) is a snapshot of a storyline, and a storyline is assembled in CODE —
-- candidate set = open storylines in the article's sport sharing ≥1 resolved entity within 14d;
-- score = entity overlap + story_type match + recency; attach above threshold 2 or open a new
-- one. Free-text story names are BANNED from matching (D3's lesson): entities + type + time ARE
-- the join key, so `title` here is display-only. BGE/cosine is not rebuilt (F2).
--
-- `storyline_entities` carries D5: an entity's PART in a storyline has its own lifespan
-- (joined_at/left_at). When a storyline resolves, code names who it resolved FOR and closes
-- every other edge as 'not_the_outcome' in one stroke.
--
-- Deploy order: additive and INERT — nothing reads or writes these tables until Phase 6 ships
-- the Desk. Safety comes from inertness.

BEGIN;

CREATE TABLE IF NOT EXISTS public.storylines (
    id            bigserial PRIMARY KEY,
    sport         text NOT NULL REFERENCES public.sports(id),
    title         text,
    status        text NOT NULL DEFAULT 'open'
                  CHECK (status IN ('open','resolved','dormant')),
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    resolved_at   timestamptz,
    resolution    jsonb
);

COMMENT ON TABLE public.storylines IS
    'One running story, assembled deterministically in code (PLAN-one-rail.md §1b) — never '
    'compiled or matched by a model. Membership lives in storyline_articles; participants in '
    'storyline_entities; the compiled product is packets.';
COMMENT ON COLUMN public.storylines.title IS
    'Display only — the first member''s headline. NEVER used for matching (D3: free-text story '
    'names are banned from the join; entities + story_type + time are the join key).';
COMMENT ON COLUMN public.storylines.resolution IS
    'Written by code when status flips to resolved: what happened and for whom. Shape owned by '
    'the Phase 6 assembler.';

CREATE TABLE IF NOT EXISTS public.storyline_articles (
    storyline_id  bigint NOT NULL REFERENCES public.storylines(id) ON DELETE CASCADE,
    article_id    bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    attached_at   timestamptz NOT NULL DEFAULT now(),
    attach_method text NOT NULL CHECK (attach_method IN ('auto','backfill')),
    PRIMARY KEY (storyline_id, article_id)
);

COMMENT ON TABLE public.storyline_articles IS
    'Membership edge: which articles a storyline is made of. Attachment is deterministic and '
    'logged (§1b scoring rule); ''auto'' is the live path, ''backfill'' a deliberate repair.';

CREATE TABLE IF NOT EXISTS public.storyline_entities (
    storyline_id bigint  NOT NULL REFERENCES public.storylines(id) ON DELETE CASCADE,
    entity_type  text    NOT NULL CHECK (entity_type IN ('player','team','person')),
    entity_id    integer NOT NULL,
    sport        text    NOT NULL REFERENCES public.sports(id),
    role         text,
    joined_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    left_at      timestamptz,
    exit_reason  text,
    PRIMARY KEY (storyline_id, entity_type, entity_id, sport)
);

COMMENT ON TABLE public.storyline_entities IS
    'D5: an entity''s part in a storyline has its own lifespan. left_at IS NULL = active '
    'participant (the packet fan-out grain). On resolution, code names who the story resolved '
    'for and closes every other edge with exit_reason ''not_the_outcome'' in one stroke.';

CREATE INDEX IF NOT EXISTS idx_storyline_articles_article
    ON public.storyline_articles (article_id);
CREATE INDEX IF NOT EXISTS idx_storyline_entities_entity
    ON public.storyline_entities (entity_type, entity_id, sport, last_seen_at);
CREATE INDEX IF NOT EXISTS idx_storylines_sport_status
    ON public.storylines (sport, status, last_seen_at);

INSERT INTO public.schema_migrations(version) VALUES ('200_one_rail_storylines')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
