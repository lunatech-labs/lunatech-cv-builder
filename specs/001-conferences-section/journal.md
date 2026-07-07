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
  - **RESOLVED in 604df8c** (post-Copilot review): the Typst `none` gap is now
    fixed via the `conf-title(cv-data)` helper + a regression test. See the
    "Copilot PR review" section below. No longer open.

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

## T5 — critic PASS (after one FAIL, fixed)

- Full suite re-run by critic: 51 lib + 32 api (DB-backed, genuinely ran) + 13
  conferences = 96 tests, 0 failures. No regression. AC8 met.
- Cumulative branch diff touches only: assets/cv.typ, frontend/index.html,
  tests/conferences.rs, assets/fixtures/01-camille-dubois.yaml, docker-compose.yml
  (approved infra), docs/, specs/. src/*, migrations/, other fixtures, reviewer
  skill all unchanged vs main.
- First critic pass FAILED: T5's authored doc prose used em dashes, violating the
  org no-em-dash rule. Fixed: de-em-dashed all authored prose in the doc (kept
  only the code-fence literals: fixture `subtitle: Workshop — ...` lines and the
  real `<span class="cv-block-sep">—</span>` JS snippet). Re-verified PASS.
- Doc "Final shape (as built)" section records the three keys, defaults,
  placement behaviour, MOVE decision, no-serialiser-change, and test locations.

## DONE — all tasks complete (T1-T5 critic PASS)

## Post-DONE addition: in-browser PDF preview (T6)

### Why (user decision)

- After the conferences feature was DONE, the user wanted to visually verify the
  new render across ALL pages of a CV before opening the PR, WITHOUT having to
  download a PDF file each time. The existing UI only had "Export PDF", which
  forces a file download (fetch to blob to synthetic `<a download>` click), so
  checking the render meant a download per iteration.
- Investigation confirmed the cheapest correct path: the `/api/cvs/{id}/pdf`
  route already serves `Content-Disposition: inline` (src/handlers.rs:213) and
  the Typst template already produces true multi-page PDFs, so the browser's
  native viewer can show every page. No backend change and no PDF library are
  needed. The only real constraint is auth: an `<iframe src="/api/...">` cannot
  carry the Bearer JWT, so we reuse the SAME fetch-to-blob pattern that
  `downloadCvPdf` already uses (index.html:2195) and point the iframe at the
  `blob:` URL instead of triggering a download.
- USER DECISIONS:
  - UI = modal viewer: a "Preview PDF" button next to "Export PDF" opens a
    slide-over modal (cloned from the review-modal pattern) containing an
    `<iframe>` of the full multi-page PDF. Editor stays visible behind it.
  - Scope = add to THIS branch (spec/001-conferences-section); ship conferences
    and the preview together in one PR.
- Motivation is explicitly a testing/QA affordance for the conferences render
  (see the modal viewer so all pages are visible in-browser), but it is a
  general feature that works for any CV.

### Scope (T6)

- Frontend-only (`frontend/index.html`): CSS for a `.pdf-modal` (reusing
  `.review-modal-bg` / `.visible` mechanics, wider panel, full-bleed iframe),
  modal markup with an `<iframe>`, a `previewPDF()` handler
  (save-if-writable then fetch-to-blob then set iframe src then open) and
  `closePdfPreview()` (revoke the blob URL and clear the iframe on close), and a
  "Preview PDF" trigger button in the editor toolbar.
- NO backend change (route already serves inline), NO new dependency (browser
  native PDF viewer), NO change to the conferences feature.

### Acceptance criteria (T6)

- [x] T6-AC1: A "Preview PDF" control is present in the editor toolbar and opens
  a modal showing the CV's PDF rendered in-browser (no file download triggered).
- [x] T6-AC2: The preview shows ALL pages of a multi-page CV (scrollable), not
  just page one, and reflects the currently selected theme.
- [x] T6-AC3: Closing the modal revokes the object URL (no blob leak) and clears
  the iframe; reopening works repeatedly.
- [x] T6-AC4: Auth-safe — the PDF is fetched through the auth-wrapped fetch (same
  as downloadCvPdf), so it works in Keycloak mode; no plain `<iframe src=/api>`.
- [x] T6-AC5: No regression — existing Export PDF still downloads; the full test
  suite still passes (frontend is a single static file, so JS behaviour is
  verified by driving the running dev server, not an automated JS test).

### Verification plan (critic MUST verify)

- This addition goes through the SAME critic gate as T1-T5, adapted for a
  frontend-only change with no JS test harness:
  1. Diff scope: only `frontend/index.html` (+ this journal / spec) changed; no
     backend, no new CDN script tag, no dependency.
  2. Live drive against the running dev server (currently on 127.0.0.1:3001,
     alt PG on 5434 per the user's port request): open the modal on a CV,
     confirm the PDF renders in the iframe, confirm multi-page scroll, confirm
     the correct theme, confirm NO download is triggered, confirm close revokes
     the blob URL (e.g. iframe src cleared / performance.getEntriesByType or a
     re-open works), confirm Export PDF still downloads.
  3. Regression: `cargo test` still green (unchanged; no backend touched).
  4. Report PASS/FAIL with evidence (screenshots or DOM/network observations).

### T6 — critic PASS (verified live in headless Chrome)

- Stage A (diff/code): only `frontend/index.html` (+ journal) changed. No
  backend, no new CDN `<script src=>`, no dependency. `previewPDF()` uses the
  auth-wrapped `fetch('/api/cvs/{id}/pdf?theme='+currentTheme)` (same path as
  downloadCvPdf), consumes `res.blob()` and sets `iframe.src` to a blob URL (no
  `<a download>`, no `<iframe src="/api">`). `closePdfPreview()` revokes the blob
  and resets to about:blank. Save-if-writable guard matches exportPDF.
- Stage B (live drive): clicked the real "Preview PDF" button on Camille Dubois.
  Modal opened with `.visible`; iframe src = `blob:...`; NO download fired
  (`downloadWillBegin` never triggered). Blob is `application/pdf`, `%PDF-`,
  198 KB, `/Count 2` (2 pages). Chrome native viewer rendered both pages ("1 / 2"
  pager + thumbnail rail), showing the branded CV incl. the conferences render.
  On close: iframe = about:blank, old blob URL revoked (`fetch(oldUrl)` rejects),
  tracker nulled; re-open produced a fresh, different blob URL (no stale state).
  Export PDF button + exportPDF/downloadCvPdf still present/functions.
- Stage C (regression): `cargo test --test conferences` 13/13 pass; no backend
  touched.
- All T6-AC1..AC5 satisfied.

## Copilot PR review (PR #23) — 4 comments, all addressed

Reviewed each Copilot inline comment; verified claims empirically before acting.

- **#1 & #2 (assets/cv.typ empty `conferences_title`)**: Copilot claimed it can
  *fail the PDF render*. Empirically it does NOT crash (both side and main render
  valid PDFs), but it DID render a BLANK heading instead of falling back to
  "Conferences" (empty-title PDF was ~5KB smaller than missing-title). Real but
  cosmetic bug (the same `none` gap the T1 critic flagged). FIXED: added a
  `conf-title(cv-data)` helper that treats both `none` and `""` as missing and
  falls back to "Conferences"; both guards now call it. (`opt` returns `""` for a
  missing key and `none` for a present-but-empty key; the helper covers both.)
- **#4 (tests/conferences.rs regression test)**: ADDED
  `conferences_empty_title_falls_back_to_default`. A plain `%PDF-` assertion would
  NOT catch the blank-heading bug (the buggy version also compiled), and the
  heading text is compressed in the PDF stream (no PDF-parsing dep in scope).
  Instead the test asserts behavioural equivalence: empty-title output ==
  missing-title output (both "Conferences"), and != a custom title. With the fix
  those two are byte-identical (verified diff=0); a regression would diverge them.
- **#3 (frontend/index.html spinner)**: real minor bug. The error/close paths set
  `loading.textContent`, destroying the spinner `<span>`, so the 2nd+ open showed
  no spinner. FIXED: `previewPDF()` now rebuilds `loading.innerHTML` (spinner +
  text) on every open; removed the redundant `textContent` reset in
  `closePdfPreview()`. Verified live in headless Chrome: spinner present on both
  open#1 and open#2, PDF loads both times.
- Result: `cargo test --test conferences` 14/14 pass (was 13, +1 regression test).

## DONE — conferences (T1-T5) + in-browser PDF preview (T6) + Copilot fixes, all verified
