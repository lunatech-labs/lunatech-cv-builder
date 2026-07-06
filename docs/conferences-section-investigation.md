# Adding a "Conferences / Speaker" section

Investigation into whether a conferences section exists, and what it takes to add
one. All claims below are cited to the source files that own them (verified by
reading the code, not a summary of it).

## Short answer

There is **no** dedicated `conferences` section today, but the pattern for adding
one already exists and is cheap to copy. A `noteworthy` section already models
speaking engagements (name + subtitle + description + tags), and conference talks
are put there in the fixtures. See `assets/fixtures/01-camille-dubois.yaml:146`
(`noteworthy:` with `subtitle: Speaker`, a KubeCon talk).

Adding a first-class `conferences` section is a two-file change (HTML preview +
Typst template) because the Rust serialiser is fully generic and passes any YAML
key through untouched.

## A note on the proposed YAML shape

The draft in the request uses ad-hoc keys (`conf 1:`, `conf 2:`) and mixes in a
`name:` on one entry. That will not render as intended:

- The renderers read fixed field names per entry. The block renderers read
  `name`, `subtitle`, `description`, `tags` (`frontend/index.html:1550`,
  `assets/cv.typ:331`). Keys like `conf 1` / `conf 2` are not read by anything,
  so they would be silently dropped from both the HTML preview and the PDF.
- `conf 1` / `conf 2` also contain a space, which is legal YAML but reads as two
  separate conferences squeezed into one entry.

Because a `noteworthy` section already exists and renders in both the HTML
preview and the PDF (see below), the zero-code option is to put conference talks
there directly. The schema-consistent shape is one entry per talk, reusing the
existing block fields (`name`, `subtitle`, `description`, `tags`). Here is the
excerpt from the request transformed into valid `noteworthy` entries:

```yaml
# ── NOTEWORTHY ───────────────────────────────
noteworthy:
  - name: DevQuest Niort 2026 / Agile Pays Basque 2025
    subtitle: Workshop — Agile Product Delivery
    description: >
      Hands-on session (10-30 participants) focusing on the role of the PO,
      backlog refinement, and bridging the gap between vision and technical
      execution.
    tags: [Agile, Product Ownership, Facilitation]

  - name: DevQuest Niort 2025 / Pix Event 2024
    subtitle: Workshop — Agile Transformation
    description: >
      Strategies for organizational change (20-65 participants), used both as a
      public talk and as an internal training program for new Scrum Masters.
    tags: [Agile, Transformation, Coaching]

  - name: Le Camping des Speakers 2023
    subtitle: Talk — The Scrum Master Role
    description: >
      Talk (40 participants) on human-centric leadership, revitalizing team
      rituals, and the transition from developer to facilitator.
    tags: [Scrum, Leadership]

  - name: Sunny Tech Montpellier 2022 / Le Camping des Speakers 2022
    subtitle: Workshop — Agile Discovery
    description: >
      Collaborative techniques (15-20 participants) to define product value and
      user needs before entering the development cycle.
    tags: [Agile, Discovery, Product]
```

Transformation notes:

- `conf 1` / `conf 2` are not read by any renderer, so they are folded into a
  single `name` with a `/` separator to credit both co-located events on one
  card (rather than inventing `conf 1` / `conf 2` keys).
- The workshop/talk title moves to `subtitle` (the italic sub-line the renderers
  show next to the name).
- Participant counts stay in `description` — there is no dedicated field for them.

If you add a *dedicated* `conferences:` section instead of reusing `noteworthy`,
use the exact same per-entry shape (`name` / `subtitle` / `description` / `tags`)
and wire the two renderers as described below.

## How sections work across the stack

Sections are all optional and independent — each renderer guards on the key being
present, so an absent section simply does not render (see the `noteworthy` guards
below).

### 1. YAML fixtures / template

- Fixtures: `assets/fixtures/01-camille-dubois.yaml`,
  `02-alice-marin.yaml`, `03-tomas-petit.yaml`; canonical template
  `assets/cv-empty.yaml`.
- Top-level keys seen across fixtures: `name`, `title`, `lunatech_since`,
  `client_name`, `key_assets`, `summary`, `experiences`, `projects`,
  `noteworthy`, `skills`, `education`, `certifications`, `languages` (plus
  `email`, `availability`, `location`, `theme` read by the renderers).
- The template for a conferences entry is `noteworthy`, defined at
  `assets/fixtures/01-camille-dubois.yaml:146`:

  ```yaml
  noteworthy:
    - name: KubeCon Europe 2024
      subtitle: Speaker
      description: >
        Presented "eBPF-powered SLOs at Scale" to an audience of 400+, ...
      tags: [eBPF, Cilium, Kubernetes, SLOs]
  ```

### 2. HTML preview — `frontend/index.html`

`renderCV(data)` builds each section into a string variable, then concatenates
the ones that are non-empty into `root.innerHTML`.

- The `noteworthy` map block to copy — `frontend/index.html:1550`:

  ```js
  var noteworthy = (data.noteworthy||[]).map(function(p) {
    var headline = '<span>' + (p.name||'') + '</span>' +
                   (p.subtitle ? '<span class="cv-block-sep">—</span><span class="cv-block-sub">' + p.subtitle + '</span>' : '');
    return '<div class="cv-block">' +
             '<div class="cv-block-title">' + headline + '</div>' +
             (p.description ? '<div class="cv-block-desc">' + descParagraphs(p.description) + '</div>' : '') +
             tagsHtml(p.tags) +
           '</div>';
  }).join('');
  ```

- The side-column injection line to copy — `frontend/index.html:1621`:

  ```js
  (noteworthy ? '<div class="cv-section"><div class="cv-section-title bullet-spark">Noteworthy</div>' + noteworthy + '</div>' : '') +
  ```

- Reusable CSS classes (`.cv-block`, `.cv-block-title`, `.cv-block-desc`,
  `.cv-block-sub`, `.cv-block-sep`) are already defined, so a new section reusing
  `cv-block` needs no new CSS. Section-title bullet variants live in CSS as
  `bullet-diamond`, `bullet-spark`, `bullet-arrow`.

To add conferences: add a `var conferences = (data.conferences||[]).map(...)`
block (copy `1550`) and one injection line in the side column (copy `1621`),
pointing the section title at "Conferences".

### 3. Typst PDF template — `assets/cv.typ`

Same shape: a `project-block` helper renders name/subtitle/description/tags, and
each side-column section is an `if opt-arr(...).len() > 0 { ... }` guard.

- The shared helper — `assets/cv.typ:331` (`project-block`, reads `name`,
  `subtitle`, `description`, `tags`).
- The `noteworthy` render to copy — `assets/cv.typ:428`:

  ```typst
  if opt-arr(cv-data, "noteworthy").len() > 0 {
    section-title("Noteworthy", bullet: "★")
    for item in cv-data.noteworthy { project-block(item) }
    v(2mm)
  }
  ```

  This sits inside the SIDE COLUMN block of the top-level two-column grid
  (`assets/cv.typ:405`–`457`).

To add conferences: paste a copy of `428`–`432` into the side column, pointing
at `cv-data.conferences` and titled "Conferences". No new helper needed —
`project-block` already handles the fields.

> Two-renderer rule (`CLAUDE.md`): the HTML preview and the Typst template are
> independent renderers of the same schema. Any visual change must land in both.

### 4. Rust serialiser — `src/pdf.rs` (no change needed)

`render()` (`src/pdf.rs:105`) parses the YAML into `serde_yaml::Value`, then
`write_value()` (`src/pdf.rs:130`) recursively serialises **every** mapping
key/value into a Typst dict literal — the Mapping arm iterates all pairs with no
hardcoded key names. The only special-cased key is `theme`. A new `conferences:`
key therefore appears automatically as `cv-data.conferences` in the generated
Typst source with no serialiser edit.

### 5. Seniority scoring — `src/seniority.rs` (optional change)

`score_yaml()` (`src/seniority.rs:95`) only reads `experiences`, `projects`, and
`title`. It does not read `noteworthy` or any other side-column section.

`score_external()` (`src/seniority.rs:409`) builds a text blob from **project**
names/descriptions and **experience** descriptions only, then counts hits against
`EXTERNAL_KEYWORDS` — which already includes `"conference"`, `"speaker"`,
`"talk"`, `"keynote"`, `"presented"`, etc.

Implication: a new `conferences` section will **not** boost the external-signals
seniority dimension unless `score_external` is extended to loop over it. Today,
conference talks only count if they appear in a project/experience description.
If conferences should count, add a loop over the `conferences` sequence in
`score_external` mirroring the projects loop at `src/seniority.rs:414`. This is
the only Rust behavioural change worth considering, and it is optional.

### 6. cv-reviewer skill — `assets/skills/cv-reviewer/SKILL.md` (no change needed)

The rubric does not mention conferences, speaking, or talks. Its 8 criteria are
project/mission-focused. A new `conferences` section requires no skill change to
function — the reviewer will simply ignore it. If we want Claude to critique
speaking history, the skill would need a new criterion.

## Summary

| Layer | File | Change |
| --- | --- | --- |
| YAML | fixtures / `assets/cv-empty.yaml` | Optional: add a `conferences:` example |
| HTML preview | `frontend/index.html:1550`, `:1621` | Add a map block + one injection line (copy `noteworthy`) |
| Typst PDF | `assets/cv.typ:428` | Add one `if/for` block (reuse `project-block`) |
| Rust serialiser | `src/pdf.rs` | None — generic pass-through |
| Seniority | `src/seniority.rs:409` | Optional — only if conferences should score |
| Reviewer skill | `assets/skills/cv-reviewer/SKILL.md` | None |

Cheapest correct implementation: model `conferences` exactly on `noteworthy`
(`name` / `subtitle` / `description` / `tags`), touching only
`frontend/index.html` and `assets/cv.typ`.
