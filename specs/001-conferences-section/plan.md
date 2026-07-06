# Implementation Plan: Conferences / Speaker section

> Companion to spec.md in this folder. Read the spec's sections 3 (scope),
> 4 (constraints), 5 (acceptance criteria) first. This plan touches only the
> two renderers, the Dubois fixture, one test file, and the investigation doc.

## Technical approach

- **Two independent renderers, one YAML schema, no serialiser change.** A
  conference entry reuses the exact field set the existing block renderers
  already read (`name` / `subtitle` / `description` / `tags`). Because
  `src/pdf.rs::write_value` is a generic pass-through (it iterates every mapping
  key with no hardcoded names, verified at `src/pdf.rs:130`), a new top-level
  `conferences:` key surfaces automatically as `cv-data.conferences` in the
  generated Typst source. No Rust serialiser edit, per spec section 3.
- **The seam in each renderer is placement selection.** Both renderers read
  three top-level keys: `conferences` (the list), `conferences_placement`
  (`main` | `side`), and `conferences_title` (heading override). A single small
  branch chooses which existing block helper to feed the list into. The default
  and the fallback are the same code path (`side`), so "absent", "empty", and
  "unrecognised" all collapse to one branch — robust by construction, not by
  three separate guards.
- **Reuse, do not invent.** HTML: the side variant reuses the `noteworthy` map
  block (`frontend/index.html:1550`) verbatim, retargeted at `data.conferences`;
  the main variant reuses the Experience section markup shape
  (`frontend/index.html:1616` / the `.cv-exp` block built at 1509-1521) but,
  because conference entries carry `name`/`subtitle` not `company`/`role`/
  `period`, it is cleanest to render the main variant with the same
  `.cv-block` markup wrapped in a full `.cv-section` in the main column (styled
  as a main-column section like Experience, without inventing timeline markers
  for data that has no periods). Typst: the side variant reuses `project-block`
  (`assets/cv.typ:331`); the main variant reuses `project-block` too, emitted in
  the MAIN column block so it inherits main-column width/insets. Both use the
  existing `section-title(name, bullet:)` helper (`assets/cv.typ:206`) and the
  `opt` / `opt-arr` guards (`assets/cv.typ:96-97`).
- **Title + placement read once, defensively.** HTML:
  `var confTitle = data.conferences_title || 'Conferences';` and
  `var confPlacement = (data.conferences_placement === 'main') ? 'main' : 'side';`
  — anything that is not exactly `'main'` becomes `'side'`. Typst:
  `let conf-title = if opt(cv-data, "conferences_title") != "" { cv-data.conferences_title } else { "Conferences" }`
  and `let conf-main = opt(cv-data, "conferences_placement", default: "side") == "main"`.
- **Fixture move, not duplicate** (spec Open Question resolved to MOVE): the four
  workshop/talk entries currently in the Dubois `noteworthy` block
  (`assets/fixtures/01-camille-dubois.yaml:163-191`) move into a new
  `conferences:` block. The two genuinely non-conference entries (KubeCon
  Speaker line 147, CNCF TAG Contributor line 155) stay in `noteworthy`. Add
  `conferences_placement:` and optionally `conferences_title:` to the fixture so
  it demonstrates the feature.
- **Automated coverage lives at the real production seam.** `cv_builder::pdf::render(yaml, theme)`
  is the exact function the PDF HTTP handler calls (`src/handlers.rs:196`) and is
  publicly exported via `src/lib.rs`. A new test file `tests/conferences.rs`
  calls it directly with conferences YAML in each placement and asserts the
  output starts with `%PDF-` — this drives the whole serialiser → template →
  compile path, the same one users hit. Tests go in a NEW file so `src/pdf.rs`
  is not modified (spec section 3 forbids editing it).

## Concrete edit points

### HTML renderer — `frontend/index.html`

1. **Build block strings** inside `renderCV(data)`, next to the existing
   `noteworthy` block (after `frontend/index.html:1558`). Add:
   - `var confTitle = data.conferences_title || 'Conferences';`
   - `var confPlacement = (data.conferences_placement === 'main') ? 'main' : 'side';`
   - `var conferences = (data.conferences||[]).map(function(p) { ... });` — copy
     the exact body of the `noteworthy` map (lines 1550-1558): headline with
     `name` + optional `subtitle`, `.cv-block` wrapper, optional
     `.cv-block-desc` via `descParagraphs`, `tagsHtml(p.tags)`. This satisfies
     AC6 (all four fields render; a name-only entry renders no empty
     sub-elements because each is guarded by a ternary).
2. **Main-column injection** — in the `cv-col-main` div, immediately after the
   Experience line (`frontend/index.html:1616`), add:
   `(conferences && confPlacement === 'main' ? '<div class="cv-section"><div class="cv-section-title">' + esc(confTitle) + '</div>' + conferences + '</div>' : '') +`
   This renders conferences as a full main-column section after Experience
   (spec Open Question: "immediately after Experience" confirmed against
   layout). AC2.
3. **Side-column injection** — in the `cv-col-side` div, next to the Noteworthy
   line (`frontend/index.html:1621`), add:
   `(conferences && confPlacement !== 'main' ? '<div class="cv-section"><div class="cv-section-title bullet-spark">' + esc(confTitle) + '</div>' + conferences + '</div>' : '') +`
   AC1 (side default), AC3 (title honoured), AC5 (fallback → side).
   Because both injections gate on `confPlacement`, a conferences list renders in
   exactly one column, never both.
4. **Absent section** (AC4): `conferences` is `''` when `data.conferences` is
   absent (the `||[]` + `.join('')` produce an empty string), so both ternaries
   short-circuit and nothing renders — no empty heading, no error. No code
   needed beyond the guards above.

### Typst renderer — `assets/cv.typ`

1. **Main column** — inside the MAIN COLUMN block, immediately after the
   Experience loop (`assets/cv.typ:398-401`, before the block closes at 402),
   add:
   ```
   if opt-arr(cv-data, "conferences").len() > 0 and opt(cv-data, "conferences_placement", default: "side") == "main" {
     v(3mm)
     section-title(if opt(cv-data, "conferences_title") != "" { cv-data.conferences_title } else { "Conferences" })
     for item in cv-data.conferences { project-block(item) }
   }
   ```
   AC2. Reuses `section-title` (default `◆` bullet, matching Experience) and
   `project-block`.
2. **Side column** — inside the SIDE COLUMN block, next to the Noteworthy guard
   (`assets/cv.typ:428-432`), add:
   ```
   if opt-arr(cv-data, "conferences").len() > 0 and opt(cv-data, "conferences_placement", default: "side") != "main" {
     section-title(if opt(cv-data, "conferences_title") != "" { cv-data.conferences_title } else { "Conferences" }, bullet: "★")
     for item in cv-data.conferences { project-block(item) }
     v(2mm)
   }
   ```
   AC1, AC3, AC5. The `!= "main"` guard makes absent / empty / bogus placement
   all render on the side (AC5); `opt-arr(...).len() > 0` makes an absent section
   render nothing (AC4). `project-block` already guards each optional field via
   `opt` / `opt-arr` (AC6).
3. Both guards read `conferences_placement` through `opt(..., default: "side")`,
   so a missing key defaults to side and an unknown value fails the `== "main"`
   test and passes the `!= "main"` test — exactly one column renders.

### Fixture — `assets/fixtures/01-camille-dubois.yaml`

1. Remove the four entries at lines 163-191 (DevQuest/Agile, DevQuest/Pix,
   Le Camping 2023, Sunny Tech/Le Camping 2022) from `noteworthy`.
2. Add a new top-level `conferences:` block (place it after `noteworthy`, before
   `skills` at line 193) containing those four entries verbatim, plus a comment
   banner matching the file's style.
3. Add `conferences_placement: side` (or `main`) and optionally
   `conferences_title:` to demonstrate the keys. Recommend `side` so the fixture
   exercises the default path visibly; the automated test covers `main`.
   (KubeCon Speaker + CNCF Contributor stay in `noteworthy`.)

### Test — `tests/conferences.rs` (new file)

Direct calls to `cv_builder::pdf::render(yaml, "lunatech")` (no Postgres, no
HTTP needed — this is a plain `#[test]`, not `#[sqlx::test]`). Cases:
- conferences with no placement key → renders (`%PDF-`), AC1/AC7.
- `conferences_placement: main` → renders, AC2.
- `conferences_placement: bogus` and empty value → renders (does not error),
  AC5.
- `conferences_title: "Speaking & Workshops"` → renders, AC3.
- an entry with only `name` (no subtitle/description/tags) → renders, AC6.
- the actual Dubois fixture string (read via `include_str!` or inline) → renders,
  AC7/AC8.
Each asserts `bytes.starts_with(b"%PDF-")` and a reasonable length. Assertion
values come from the Typst compiler succeeding, an independent source of truth
(the compiler), not from the code under test recomputing anything.

### Doc — `docs/conferences-section-investigation.md`

Add a short "Final shape" note at the end recording the chosen keys
(`conferences`, `conferences_placement`, `conferences_title`), that the entries
were MOVED not duplicated, and the side default / fallback behaviour, so the doc
stays accurate per spec section 3.

## Testing strategy and coverage per AC

| AC | What it asserts | Automated (Typst/PDF) | HTML |
| --- | --- | --- | --- |
| AC1 side default | side mini section, heading "Conferences" | `render` succeeds with no placement key (compile + %PDF-) — proves the side branch compiles and is taken | visual only |
| AC2 main placement | full main-column section | `render` with `placement: main` succeeds | visual only |
| AC3 title | custom heading string used | `render` with `conferences_title` succeeds | visual only |
| AC4 absent | nothing renders, no error | existing full-schema tests already render CVs with no `conferences`; add an explicit no-conferences case | visual only |
| AC5 fallback | bogus/empty → side, no error | `render` with `placement: bogus` and empty succeeds | visual only |
| AC6 fields | 4 fields render; name-only renders clean | `render` with a full entry and a name-only entry both succeed | visual only |
| AC7 PDF compiles | Dubois fixture with conferences compiles | direct `render` on the fixture YAML → %PDF- | n/a |
| AC8 no regression | `cargo test` green; only intended fixture change | full `cargo test` run; the existing `tests/api.rs` PDF tests + `src/pdf.rs` unit tests must still pass | n/a |

## Coverage limitations (headline for the supervisor at Gate 2)

- **The HTML renderer is verified by eye only.** `frontend/index.html` is a
  single static file with no JS test harness, no build step, and no DOM test
  runner in this repo (confirmed — the only tests are Rust: `src/pdf.rs` unit
  tests and `tests/api.rs` integration tests). Every AC's HTML half (the
  "in both renderers" clause of AC1-AC6) can only be checked by loading a CV in
  the browser preview and looking. The plan does NOT add automated HTML
  coverage, and adding a JS test toolchain is out of scope (spec: "frontend
  stays a single static HTML page, no build step"). The implementer/critic must
  do a manual browser pass for AC1-AC6 on the HTML side.
- **The automated PDF assertion is "compiles and emits a PDF", not "looks
  right".** `render` returning `%PDF-` proves the Typst branch was reached, the
  serialised YAML was valid Typst, and the template compiled — it does NOT prove
  the section landed in the correct column, used the right heading text, or is
  styled like Experience vs Noteworthy. We do not re-parse the PDF. So the
  Typst side of AC1/AC2/AC3 (column choice, styling, heading text) is only
  *partially* covered: "did not error" is automated, "rendered in the right
  place with the right label" is visual. This matches the ceiling of the
  existing pdf tests (see `render_works_for_all_themes`, which likewise only
  asserts `%PDF-`).
- **Net:** AC7 and AC8 are fully automated. AC1-AC6 have automated "does not
  error / compiles" coverage on the Typst path and require manual visual
  confirmation for the placement/styling/heading assertions and for the entire
  HTML path.

## Risks & pitfalls

- **Typst `if/else if` newline gotcha** (CLAUDE.md): do not chain the placement
  decision as a multi-line `if/else if` expression — the newline ends the `if`.
  Use two independent `if` guards (as written above) or a single-line
  conditional, not a chained expression.
- **`opt` default semantics**: `opt(d, k, default: "side")` returns `"side"`
  when the key is absent; but an *empty string* value (`conferences_placement:`)
  parses as `none` in YAML → Typst, and `opt-arr`/`opt` handle `none`. Confirm
  an empty value does not equal `"main"` (it won't) so it falls to side. The
  test's empty-value case pins this.
- **Two-renderer drift**: the main-column HTML variant must be styled like a
  main-column section (like Experience) and the Typst main variant likewise;
  keep the *intent* aligned even though markup differs. A task that edits one
  renderer without the other violates the two-renderer rule — the task
  breakdown pairs them.
- **Fixture is git-dirty already**: `assets/fixtures/01-camille-dubois.yaml`
  shows as modified in git status at branch start. The implementer must diff
  carefully so the conferences move is the only intended change and does not
  clobber whatever the pre-existing modification is. Flag to check `git diff`
  on that file before editing.
- **`src/pdf.rs` is off-limits**: its `#[cfg(test)] mod tests` is inside the
  file, so new tests go in `tests/conferences.rs`, not there.

## Open questions

- Fixture demonstration placement: render the Dubois `conferences` as `side`
  (shows the default) or `main` (shows the headline variant)? Plan assumes
  `side`; trivial to flip. Not blocking.
- No other ambiguities; the two spec Open Questions (vertical placement = after
  Experience; MOVE not duplicate) are resolved above and reflected in the tasks.
