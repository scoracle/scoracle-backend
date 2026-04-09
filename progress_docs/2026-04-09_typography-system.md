# 2026-04-09 Typography System Spec

## Goal

Capture the Scoracle frontend typography system in the data repo so the Astro
frontend has a single source of truth to pull from. The data repo already hosts
frontend-facing design docs (`BOOTSTRAP_FRONTEND.md`, `FRONTEND_INTEGRATION.md`),
so typography fits the same pattern.

## Decisions

- **Headers / display**: Tan Nimbus 400 (Fontshare). Single-weight face — never
  apply bold.
- **Body / UI**: DM Sans (Google Fonts). Light (300) is the workhorse body
  weight; 400 for UI labels; 500–600 reserved for CTAs and metadata.
- **Tokens**: exposed as `--font-display`, `--font-body`, and
  `--weight-{light,regular,medium,semibold}` custom properties so components
  reference tokens instead of hard-coded font names.
- **Integration surface**: font `<link>` tags load in `BaseLayout.astro` with
  `preconnect` hints to `api.fontshare.com`, `fonts.googleapis.com`, and
  `fonts.gstatic.com`.
- **Aesthetic target**: midpoint between Anthropic (warm, human) and NYT / The
  Athletic (minimal, editorial). Tan Nimbus's ethereal stroke matches the
  tattoo flash-style crystal ball logo; DM Sans grounds the rest of the UI.

## Accomplishments

- Added `planning_docs/TYPOGRAPHY_SYSTEM.md` with font choices, embed snippet,
  CSS tokens, usage guardrails (tracking, weight rules, eyebrow style), Astro
  layout integration, and guidance on where to drop the tokens in the frontend.

## Quick Reference

- Spec: `planning_docs/TYPOGRAPHY_SYSTEM.md`
- Frontend target file when adopted: `src/layouts/BaseLayout.astro` +
  `src/styles/global.css` in the Astro repo.
- Fonts: Tan Nimbus (Fontshare, free commercial) + DM Sans (Google Fonts).

## Notes

No backend (Go, Python, SQL) code was touched — this change is design
documentation only. Frontend adoption happens in the Astro repo by copying the
`<link>` tags into `BaseLayout.astro` and the `:root` block into the global
stylesheet.
