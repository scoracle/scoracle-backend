---
name: skill-investigator
description: Resolve candidate identities and enrich existing player records using structured Wikimedia evidence.
---

For a candidate name, use the Wikidata search and item readers in `scripts/adapter/discover.rs`. Apply the sport and role checks in `scripts/cognition/gate.rs` before recording an alias or creating a person. For an existing player, verify identity against career-team evidence before attaching sourced facts. Record the source document and acquisition outcome for each decision.

The historical prose evaluation uses [the verbatim extraction prompt](references/prose-extraction.md). The structured adapter owns publication.
