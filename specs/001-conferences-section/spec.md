# Feature Spec: Conferences / Speaker section

> Status: SPECIFIED
> Spec folder: specs/001-conferences-section/

## 1. Mission / Why

Consultants who speak at conferences, run workshops, or give talks have no
first-class place to record it in a CV. Today speaking is shoehorned into the
`noteworthy` mini section. We want a dedicated `conferences` section that a
recruiter/author can render either as a prominent main-column section (for
consultants whose speaking is a headline asset) or as a compact side-column mini
section (like `noteworthy` / `certifications`) when it is supporting detail.

## 2. Outcome

An author adds a `conferences:` list to a CV's YAML (entries with
`name` / `subtitle` / `description` / `tags`, the same shape as `noteworthy`).
By setting an optional `conferences_placement:` key they choose where it renders:
`side` (default) shows a compact mini section in the side column styled like
`noteworthy`; `main` shows a full section in the main column styled like
Experience. An optional `conferences_title:` key overrides the heading
(default "Conferences"). The section renders identically in intent in both the
HTML live preview and the Typst PDF. If `conferences` is absent, nothing renders
and no existing CV changes.

## 3. Scope

### In scope

- New top-level YAML section `conferences:` — a list of entries, each with
  `name`, optional `subtitle`, optional `description`, optional `tags` (the
  exact field set the existing `project-block` / noteworthy renderer reads).
- Optional top-level `conferences_placement:` key, values `main` | `side`,
  default `side` when absent or unrecognised.
- Optional top-level `conferences_title:` key (string), default `"Conferences"`.
- HTML preview renderer (`frontend/index.html`): render conferences in the main
  column (styled like Experience) when placement is `main`, else as a mini
  side-column section (styled like Noteworthy). Honour the title key.
- Typst PDF renderer (`assets/cv.typ`): same two placements and title, matching
  the HTML intent per the two-renderer rule.
- Example `conferences` entries in the Camille Dubois fixture
  (`assets/fixtures/01-camille-dubois.yaml`) demonstrating the section. (The
  four transformed conference entries currently sitting in that fixture's
  `noteworthy` block move into the new `conferences` section.)
- Keep `docs/conferences-section-investigation.md` accurate if the final shape
  diverges from what it documents.

### Out of scope

- No change to `src/pdf.rs` — the YAML→Typst serialiser is generic and already
  passes any key through; it must NOT gain conference-specific logic.
- No change to seniority scoring (`src/seniority.rs`) — conferences will NOT
  feed the external-signals score in this feature. (Documented as a deliberate
  non-goal; revisit separately if wanted.)
- No change to the cv-reviewer skill / rubric (`assets/skills/cv-reviewer/`) —
  the reviewer will ignore the section, which is acceptable.
- No new database column, migration, or API endpoint — conferences live inside
  the existing `yaml` blob; no schema change.
- No editor UI affordance (buttons/form fields) for conferences beyond typing
  YAML — authors edit YAML as they do for every other section.
- No auto-placement heuristic based on data shape — placement is the explicit
  YAML flag only.

## 4. Constraints & Decisions

- **Two-renderer rule** (CLAUDE.md): HTML preview and Typst template are
  independent renderers of the same schema. Every visual change lands in BOTH;
  they share intent, not code.
- **Reuse existing renderers/helpers.** In HTML reuse the `noteworthy` map
  block pattern (`frontend/index.html:1550`) for the side variant and the
  Experience section pattern (`frontend/index.html:1616`) for the main variant.
  In Typst reuse `project-block` (`assets/cv.typ:331`) for the side variant and
  `exp-block` (used by Experience) for the main variant, plus the existing
  `section-title` helper and `opt` / `opt-arr` guards.
- **Placement default is `side`** and must be robust: absent, empty, or any
  unrecognised value falls back to `side`.
- Frontend stays a single static HTML page, no build step (CLAUDE.md).
- PDF stays in-process via the `typst` crate (CLAUDE.md).
- Placement key name: `conferences_placement`; title key: `conferences_title`.
- Section rendered on the SIDE goes near noteworthy/certifications; on MAIN it
  goes in the main column after Experience.
- Decisions locked at Gate 1: placement = per-CV YAML flag; main style = like
  Experience; title = configurable via `conferences_title`.

## 5. Acceptance Criteria (how you'll verify it)

- [ ] AC1 (side default): Given a CV YAML with a `conferences:` list and no
  `conferences_placement` key, when the CV is rendered, then conferences appear
  as a mini section in the SIDE column styled like Noteworthy, in both the HTML
  preview and the PDF, with heading "Conferences".
- [ ] AC2 (main placement): Given a CV YAML with `conferences_placement: main`,
  when rendered, then conferences appear as a full section in the MAIN column
  styled like Experience, in both the HTML preview and the PDF.
- [ ] AC3 (configurable title): Given `conferences_title: "Speaking & Workshops"`,
  when rendered in either placement, then that string is the section heading
  (not "Conferences"), in both renderers.
- [ ] AC4 (absent section): Given a CV YAML with NO `conferences` key, when
  rendered, then no conferences section appears anywhere and all other sections
  render unchanged, in both renderers (no empty heading, no error).
- [ ] AC5 (placement fallback): Given `conferences_placement: bogus` (or an
  empty value), when rendered, then it falls back to the SIDE mini section (does
  not error, does not render in main), in both renderers.
- [ ] AC6 (entry fields): Given a conference entry with `name`, `subtitle`,
  `description`, and `tags`, when rendered in either placement, then all four
  fields display; and given an entry with only `name`, then it renders without
  error and without empty sub-elements. Verified in both renderers.
- [ ] AC7 (PDF compiles): Given the Camille Dubois fixture with the new
  `conferences` section, when a PDF is generated via the existing compile path,
  then the render succeeds (no Typst compile error) — covered by the pdf unit
  tests / an integration test asserting non-empty PDF bytes.
- [ ] AC8 (no regression): The existing test suite (`cargo test`) still passes,
  and no existing fixture's rendering changes except the intended move of the
  four conference entries out of `noteworthy` into `conferences` in the Dubois
  fixture.

## 6. Task Breakdown

<!-- Filled in by sdd-planner, approved by the user at Gate 2. -->

## 7. Open Questions

- Exact vertical placement of the MAIN-column conferences block relative to
  Experience (immediately after Experience assumed). Planner to confirm against
  the existing main-column layout; low-risk, cosmetic.
- Whether the four conference entries should be duplicated (kept in noteworthy
  AND added to conferences) or MOVED. Spec assumes MOVED to avoid double display;
  confirm at Gate 1 if you'd rather keep both.
