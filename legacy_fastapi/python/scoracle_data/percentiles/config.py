"""
Percentile configuration — migrated to Postgres.

All stat definitions, inverse stat flags, percentile eligibility, and
position groups now live in the stat_definitions table (migration 004).
The recalculate_percentiles() SQL function reads directly from that table
(migration 005). No Python-side configuration is needed.

This file is kept as a stub to document the migration. It can be removed
once all references are cleaned up.
"""
