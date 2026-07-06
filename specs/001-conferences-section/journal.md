# Journal — 001-conferences-section

Append-only log of decisions, drift, and critic verdicts.

## Gate 1 — SPECIFIED (approved)

- Feature: dedicated `conferences:` YAML section, renderable in main column
  (styled like Experience) or side mini section (styled like Noteworthy),
  controlled by per-CV `conferences_placement: main | side` (default `side`).
- Title configurable via `conferences_title:` (default "Conferences").
- Entry shape reuses noteworthy: `name` / `subtitle` / `description` / `tags`.
- Decisions locked (user answers at Gate 1):
  - Placement control = per-CV YAML flag.
  - Main-column style = like Experience.
  - Section title = configurable (`conferences_title`).
- Open question #1 resolved by user: MOVE the four conference entries out of the
  Dubois fixture's `noteworthy` block into the new `conferences` section (not
  duplicate).
- Out of scope confirmed: no src/pdf.rs change, no seniority change, no reviewer
  skill change, no DB/API change, no editor UI, no auto-placement heuristic.
- Pre-existing uncommitted groundwork folded into the Gate 1 baseline: the
  investigation doc (`docs/conferences-section-investigation.md`) and the four
  conference entries currently in the Dubois fixture's `noteworthy` block (they
  will be moved into `conferences` during IMPLEMENT).
