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

## Gate 2 — PLANNED (approved)

- plan.md written; spec.md section 6 filled with 5 tasks (T1-T5).
- Approach: reuse existing block helpers (noteworthy/project-block), no
  src/pdf.rs change, placement seam collapses absent/empty/bogus → side.
- User decision at Gate 2: fixture demonstrates `conferences_placement: side`
  (the default variant), not `main`.
- Coverage limitation acknowledged by user at Gate 2:
  - HTML renderer has NO automated test harness (single static file, no build
    step) — HTML half of AC1-AC6 is manual-browser verification only.
  - PDF tests assert `%PDF-` (compiles + reached), not visual correctness; we do
    not re-parse the PDF. Typst side of AC1-AC3 is "does not error" automated,
    placement/heading/styling is visual.
  - AC7, AC8 fully automated; AC1-AC6 partially automated + manual visual.
