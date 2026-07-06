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

## T1 — critic PASS

- `assets/cv.typ`: two mutually-exclusive guards (MAIN after Experience, `==
  "main"`, line ~402; SIDE by Noteworthy, `!= "main"`, line ~439). Reuses
  `project-block` + `section-title`. No chained if/else (gotcha avoided).
- `tests/conferences.rs`: 10 `#[test]`s via `pdf::render`, all `%PDF-`. 10/10
  pass (critic ran them independently). Both branches exercised.
- Only allowed files changed (critic confirmed via git diff).
- Critic note (not an AC violation, deferred hardening): an explicitly empty
  `conferences_title:` parses as YAML `none`, and `opt` returns `none` (not
  `""`), so `none != ""` is true → `none` passed to `section-title`. No AC
  requires empty-title handling. Flagged for T3/future. Watch this in the HTML
  renderer too (T3): `data.conferences_title || 'Conferences'` handles empty
  string AND null correctly, so HTML is already safe; Typst empty-title is the
  only gap and it is out of AC scope.

## T2 — critic PASS

- `tests/conferences.rs`: +2 tests (`no_conferences_key_renders_unchanged`
  with other sections present, `empty_conferences_list_renders_unchanged`).
  12/12 pass (critic re-ran). No template change needed/made.
- Red-capability real: absent-key test would fail if guard regressed, because
  the guard body does a direct `cv-data.conferences` access that errors on a
  missing key. `opt-arr` (cv.typ:97) returns `()` for absent key. Empty-list
  test is the weaker complementary boundary case.
- AC4 Typst half covered; "skipped branch cannot emit a heading" is a sound
  proxy for "no empty heading". HTML half remains for T3.

## T3 — critic PASS

- `frontend/index.html` only: conferences map block (copy of noteworthy),
  `confTitle = data.conferences_title || 'Conferences'`, `confPlacement =
  (=== 'main') ? 'main' : 'side'`, main injection (plain title, after
  Experience) + side injection (bullet-spark, near Noteworthy), mutually
  exclusive.
- Verification is static/scripted string inspection (no JS test harness exists;
  documented accepted limitation). Critic independently reproduced HTML strings
  for AC1/AC2/AC4/AC5; all 6 HTML ACs PASS.
- Two-renderer consistency confirmed vs cv.typ: placement decision, main/side
  styling (plain vs star bullet), and "Conferences" default all match. No drift.
- `esc(confTitle)` applied to user title (more careful than existing unescaped
  `client_name` at line 1603 — noted, not in scope to fix here). Empty-title
  handled correctly in HTML (falsy → fallback), so the Typst empty-title `none`
  gap from T1 does NOT exist on the HTML side.
