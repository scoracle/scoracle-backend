-- 208_boxscore_sources.sql
--
-- WHAT: the Investigator's box-score source registry (PLAN-one-rail 4.3, data-not-code).
-- One row per source FAMILY the budgeted fetcher may retrieve box scores from: which sport
-- it serves, how match pages are discovered (url_template = the surgical target-URL scrape,
-- search = discovery via a query), which parser family reads it, its trust state, and its
-- fetch budget (rpm, concurrency, cache_ttl) as data.
--
-- WHY data-not-code: adding/suspending a source must be an INSERT/UPDATE, never a deploy.
-- The provider map hardcoded in the old boxscore_fetch.rs (bdl/sportmonks — providers
-- cancelled 2026-07-27/28) is exactly the shape this table replaces.
--
-- SEEDED EMPTY, deliberately. The Phase 4.3 terms/robots review (2026-08-03, recorded in
-- the Phase 4 Log) found EVERY reviewed keyless family in both D-4 sports failing on one of:
-- technical blocks of declared bots, robots.txt disallow, JS/signature walls requiring
-- banned automation, or explicit anti-AI/anti-tool license terms. Seeding awaits Scott's
-- source ruling (the BLOCKED line in the Phase 3→4 handoff chain). The table shape is
-- family-independent and lands now so the unblocking session is an INSERT + a parser.
--
-- Deploy order: ADDITIVE and INERT — nothing reads it until junctions/investigator/
-- boxscore.rs's SourcePlan switches to it (4.5), which deploys in 4.9.

BEGIN;

CREATE TABLE IF NOT EXISTS public.boxscore_sources (
    id             bigserial PRIMARY KEY,
    sport          text NOT NULL REFERENCES public.sports(id),
    league_id      integer NULL,
    domain         text NOT NULL,
    discovery      text NOT NULL CHECK (discovery IN ('url_template', 'search')),
    url_template   text,
    parser_family  text NOT NULL,
    trust_state    text NOT NULL DEFAULT 'candidate'
                   CHECK (trust_state IN ('candidate', 'trusted', 'suspended')),
    fetch_policy   jsonb NOT NULL DEFAULT '{}'::jsonb,
    terms_review   jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (sport, domain, parser_family)
);

COMMENT ON TABLE public.boxscore_sources IS
    'Source families the Investigator may fetch box scores from (PLAN-one-rail 4.3). One '
    'adapter per family (parser_family names the Rust parser module); adding or suspending '
    'a source is data, not a deploy. A family enters as candidate, earns trusted via the '
    '4.8 replay gate, and is suspended — never deleted — when it misbehaves.';
COMMENT ON COLUMN public.boxscore_sources.league_id IS
    'NULL = the family serves the whole sport; set when a family only covers one league.';
COMMENT ON COLUMN public.boxscore_sources.discovery IS
    'url_template: match URLs are constructed/navigated from templates (the surgical '
    'target-URL scrape). search: match pages are discovered by query — Phase 5 substrate.';
COMMENT ON COLUMN public.boxscore_sources.url_template IS
    'The template(s) for url_template discovery; interpretation belongs to the parser '
    'family (it may be a match-page template or an index-page template navigated in-family).';
COMMENT ON COLUMN public.boxscore_sources.fetch_policy IS
    'Budget knobs consumed by fetch.rs::BudgetedFetcher: {"rpm": N, "concurrency": 1, '
    '"min_spacing_secs": N, "cache_ttl_secs": N}. The fetcher floors spacing at 2s '
    'regardless of what this says (the 4.2 law).';
COMMENT ON COLUMN public.boxscore_sources.terms_review IS
    'The terms/robots review that admitted this family: {"reviewed_at", "robots", "terms", '
    '"verdict", "notes"} — the provenance of the right to fetch, next to the budget.';

INSERT INTO public.schema_migrations(version) VALUES ('208_boxscore_sources')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
