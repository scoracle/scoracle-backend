# Headline entity pipeline cleanup

Implementation after the October 20 audit; not a production deployment record.

## Contract

Hourly RSS searches remain team-scoped. Every returned headline supplies candidates from its query provenance, retained canonical links and unique sport-scoped name/alias matches. Known players and every known `person` role are scored independently. A name match is a candidate, never a positive relevance decision. Colliding player/person/team IDs stay separate throughout scoring, daily membership, publication and API serving.

Migration 315 shares the candidate set between Rust and discovery revisions. It repairs current-source reconciliation, restores superseded pending obligations when a headline reverts, and does not recreate completed subject evidence after candidate-only revisions. Feed rank is queue metadata. Same-publisher headline copies occupy one writing slot while every delivery is settled.

People receive Journalist, Influencer and Insider outputs. They do not receive statistical Scout, Analyst or Oracle jobs. Reporting APIs and identity metadata accept `person`; saved products and date selection retain entity type as part of identity.

Retired code includes Harvester/Editor body-scoring replays, their old decision transport, browser-reader runtime, source-worker registration and service wiring, article scrubbing/window helpers, and Journalist's unused per-report scoring and deferral state. Existing archived article rows and output records are retained. Source compatibility fields in offline fixtures do not enable production article reading.

## Activation

Stop coverage and cognition workers between daily editions, apply migration 315, install matching Go/Rust binaries and restart the configured workers. Keep the old source-worker and browser-reader services disabled and remove their installed unit files/dependencies on the hosts; the installer no longer renders a source-worker unit. Do not roll back to an older binary against the new reader-retirement constraint.

This session changes source and tests only. Model quality, prompts/voices and production deployment are separate from plumbing validation.

## Follow-up: unknown entities

User decision: extract known entities now. In a subsequent session, design and qualify discovery and registration for people absent from the canonical database. Include unknown/unscoped names, identity ambiguity, provenance, role verification, aliases and explicit promotion rules. Do not create canonical people or infer affiliations from an unverified headline mention in this release.

## Validation

The isolated PostgreSQL 18 schema restore and migration 315 passed. The final checks passed 345 Rust library unit tests and 57 database integration tests, plus the affected Go API/RSS/database suites, Go vet, all-target Rust builds, formatting, diff checks, release/install simulation and watchdog/preflight checks.

Regression cases cover all seven non-player person roles and players extracted from team RSS, unique stored aliases, separate entity-type ID namespaces, A → B → A source recovery, rank-only and candidate-only revisions, duplicate headline selection in all three reporting characters, Journalist output guards, actual owner-card publication, reporting-only daily person assignments and outbox completion, and saved person products served with date isolation. Publication epochs now round down to whole seconds, preventing a fractional database timestamp from being misclassified as future reporting.

Live decision-model calibration and character writing fidelity were not rerun. The older offline DuckDB study tests were outside this changed pipeline. No production database, worker configuration or deployed binary was changed.
