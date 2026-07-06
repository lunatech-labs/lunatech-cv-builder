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

## T4 — critic PASS

- `assets/fixtures/01-camille-dubois.yaml`: clean MOVE (zero content edits) — four
  workshop/talk entries re-parented from `noteworthy` into a new top-level
  `conferences:` block (+ `conferences_placement: side`, `conferences_title:
  Speaking & Workshops`, comment banner). KubeCon + CNCF stay in noteworthy. Each
  moved name appears exactly once (grep -c = 4), no duplication, content byte-
  identical HEAD vs working tree.
- `tests/conferences.rs`: +`dubois_fixture_with_conferences_renders`
  (`include_str!` the real fixture → `pdf::render` → `%PDF-`). 13/13 pass.
- Critic accounted for the non-T4 uncommitted edits (XSS esc, compose fix,
  journal) and confirmed no T4 logic hidden in them.
- AC7 met (fixture compiles at production seam); AC8 met for T4's portion
  (full-suite deferred to T5).

## Security finding (background commit review) + user decisions

- Background security review of the T3 commit flagged a CROSS-USER STORED XSS in
  `frontend/index.html`. Investigation: the vuln is REAL, PRE-EXISTING, and
  REPO-WIDE. Raw YAML fields (`name`, `subtitle`, `company`, `role`, `period`,
  skill `name`) are interpolated into `innerHTML` WITHOUT `esc()` across
  `experiences`, `projects`, `noteworthy`, and `skills`. Only descriptions
  (`descParagraphs`) and tags (`tagsHtml`) are escaped. Trust model (CLAUDE.md:
  every recruiter views every consultant's CV) makes a malicious `name` in one
  CV execute in another user's browser.
- T3's conferences block faithfully COPIED the noteworthy pattern, so it added
  one more instance of the class; it did not introduce the class.
- USER DECISION (XSS scope): fix conferences ONLY now. Applied `esc()` to
  `p.name` and `p.subtitle` in the conferences map block (frontend/index.html
  ~1568). The identical pre-existing bug in noteworthy/projects/experiences/
  skills is DEFERRED — see "Follow-ups" below. Conferences is now stricter than
  its sibling sections.
- Note: this makes the conferences HTML block MORE escaped than the noteworthy
  block it was copied from. The T3 critic verified the pre-esc version; this is a
  security hardening on top. Cheap and safe (esc() only touches `& < >`).

## Infra fix (out of scope, user-approved)

- `docker-compose.yml` mounted the PG volume at `/var/lib/postgresql/data`, which
  the `postgres:18-alpine` entrypoint now REFUSES to start on (even with a fresh
  empty volume) — it wants the mount at `/var/lib/postgresql`. This blocked ALL
  DB-backed tests for anyone, not just this feature.
- USER DECISION (Postgres): approved the one-line compose fix. Changed the mount
  to `cvbuilder_pg:/var/lib/postgresql`. PG now starts and accepts connections on
  :5433. Recorded here as an approved out-of-scope infra fix. Postgres brought up
  via `podman compose` (user uses podman, not docker).

## Follow-ups (out of scope for this spec)

- **Repo-wide XSS fix**: escape `name`/`subtitle`/`company`/`role`/`period`/skill
  `name` (and the unescaped `client_name` at index.html:1603) across all sections
  of `renderCV`. Recommend a dedicated spec/issue (e.g. 002-escape-cv-fields).
  This is a real cross-user stored XSS given the shared-visibility trust model.
