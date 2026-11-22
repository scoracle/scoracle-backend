-- Findings are already parsed and source-grounded. Report index is newest first;
-- ordinality preserves Rust's stable ordering when report indices tie.
WITH findings AS (
    SELECT f.*, position
    FROM jsonb_array_elements($1::jsonb) WITH ORDINALITY AS input(value, position)
    CROSS JOIN LATERAL jsonb_to_record(value) AS f(
        counterparty text, report_index bigint, status text, stage text, publisher text
    )
), ordered AS (
    SELECT *, row_number() OVER partner AS rank,
           first_value(status) OVER partner AS lead_status
    FROM findings
    WINDOW partner AS (
        PARTITION BY counterparty COLLATE "C" ORDER BY report_index, position
    )
), runs AS (
    SELECT *, min(rank) FILTER (WHERE status <> lead_status)
        OVER (PARTITION BY counterparty COLLATE "C") AS cutoff
    FROM ordered
), active AS (
    SELECT * FROM runs
    WHERE lead_status = 'reported' AND (cutoff IS NULL OR rank < cutoff)
)
SELECT least(99, coalesce(
    max(CASE stage WHEN 'here_we_go' THEN 88 WHEN 'advanced_talks' THEN 72
                   WHEN 'concrete_interest' THEN 48 ELSE 28 END)
    + greatest(count(DISTINCT publisher COLLATE "C") - 1, 0) * 4,
    1
))::smallint
FROM active
