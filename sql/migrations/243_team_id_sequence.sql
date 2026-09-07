-- 243: teams.id gains a sequence — the self-growing layer reaches clubs.
--
-- Scott, 2026-09-06 (the Coventry/Hull debug): "When the newly promoted teams
-- get read, they should be added by automation, and then stats should flow."
-- The founding doctrine ("a league's teams already exist") left teams.id with
-- no default: every id was a vendor id, and nothing in the house could mint a
-- club. The league's own feed is the authoritative reader for league
-- membership, so the data rail becomes the club creator — and it needs ids.
--
-- The sequence starts at 20,000,000: far above the vendor id space (max
-- 13,258 at migration time) so a minted id can never collide with a vendor
-- id that arrives later in a backfill.
BEGIN;

CREATE SEQUENCE IF NOT EXISTS teams_id_seq
    AS integer
    START WITH 20000000
    OWNED BY teams.id;

ALTER TABLE teams
    ALTER COLUMN id SET DEFAULT nextval('teams_id_seq');

COMMIT;
