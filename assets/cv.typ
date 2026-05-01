// Lunatech CV — Typst template, layout cv-example.pdf.
// `cv-data` is injected as a #let by the Rust side before this file runs.

#let theme = if "theme" in cv-data { cv-data.theme } else { "lunatech" }

#let palette-lunatech = (
  accent:      rgb("#7c1818"),
  capsule-bg:  rgb("#f4e9e9"),
  header-bg:   rgb("#1a1a1a"),
  bar-fill:    rgb("#7c1818"),
  bar-empty:   rgb("#f0d8d8"),
  bullet:      rgb("#7c1818"),
  text:        rgb("#1a1a1a"),
  body:        rgb("#2a2a2a"),
  muted:       rgb("#6b6b6b"),
  border:      rgb("#d8d4d0"),
)

#let palette-luxe = (
  accent:      rgb("#0B0B0B"),
  capsule-bg:  rgb("#f5f0e8"),
  header-bg:   rgb("#0B0B0B"),
  bar-fill:    rgb("#c8a24a"),
  bar-empty:   rgb("#e8e2d4"),
  bullet:      rgb("#c8a24a"),
  text:        rgb("#1a1a1a"),
  body:        rgb("#2a2a2a"),
  muted:       rgb("#6b6b6b"),
  border:      rgb("#e8e2d4"),
)

#let palette-cosmic = (
  accent:      rgb("#0a1e50"),
  capsule-bg:  rgb("#fce7f3"),
  header-bg:   rgb("#0a1e50"),
  bar-fill:    rgb("#DB2777"),
  bar-empty:   rgb("#f0d8e8"),
  bullet:      rgb("#DB2777"),
  text:        rgb("#1a1a1a"),
  body:        rgb("#2a2a2a"),
  muted:       rgb("#6b6b6b"),
  border:      rgb("#dde0e8"),
)

#let palette-opera = (
  accent:      rgb("#9b0f17"),
  capsule-bg:  rgb("#fdf2f2"),
  header-bg:   rgb("#9b0f17"),
  bar-fill:    rgb("#ea212e"),
  bar-empty:   rgb("#f5dede"),
  bullet:      rgb("#ea212e"),
  text:        rgb("#1a1a1a"),
  body:        rgb("#2a2a2a"),
  muted:       rgb("#6b6b6b"),
  border:      rgb("#e8d8d8"),
)

#let palettes = (
  lunatech: palette-lunatech,
  luxe:     palette-luxe,
  cosmic:   palette-cosmic,
  opera:    palette-opera,
)
#let p = if theme in palettes { palettes.at(theme) } else { palette-lunatech }

// Font chains include symbol fonts at the end so glyphs like ✉ ◎ ⌖ ✦ ▸
// fall back gracefully when the primary font lacks them.
#let symbols = ("Apple Symbols", "Symbola", "Noto Sans Symbols 2", "Noto Sans Symbols", "DejaVu Sans")
// Bundled in assets/fonts/: Inter (sans) + Source Serif 4 (serif). The
// system-font fallbacks come last so a missing bundle doesn't crash the
// render — the bundle takes priority by name.
#let serif = ("Source Serif 4", "Source Serif Pro", "EB Garamond", "Garamond", "Times New Roman", "Times", "DejaVu Serif", "Liberation Serif", ..symbols)
#let sans  = ("Inter", "Poppins", "Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans", "Liberation Sans", ..symbols)

#set document(title: cv-data.name + " — CV")
// Top margin is 16mm globally so continuation pages (page 2+) start with
// breathing room. Page 1's banner bleeds into this 16mm via a `place` call
// below — see HEADER. We avoid a second `set page` mid-document because
// Typst 0.14 leaks the new top margin onto page 1, pushing the first
// project ~16mm down.
#set page(
  paper: "a4",
  margin: (top: 16mm, bottom: 16mm, x: 0pt),
  footer-descent: 6mm,
  footer: align(center, block(width: 100% - 25.4mm)[
    #line(length: 100%, stroke: 0.4pt + p.border)
    #v(1mm)
    #align(center, text(size: 6.5pt, fill: p.muted, tracking: 0.5pt)[
      Lunatech #h(2mm) #sym.dot.c #h(2mm) France #h(2mm) #sym.dot.c #h(2mm) Netherlands
    ])
  ]),
)
#set text(font: sans, size: 9pt, fill: p.text, hyphenate: true, lang: "en")
#set par(leading: 0.45em, justify: true, linebreaks: "optimized")

#let opt(d, k, default: "") = if k in d { d.at(k) } else { default }
#let opt-arr(d, k) = if k in d and d.at(k) != none { d.at(k) } else { () }

// ─────────── HEADER ───────────

// Banner content is rendered twice: once visibly via `place` to bleed into
// the page-1 top margin, and once via `hide` to reserve the same height in
// flow so subsequent content lands flush against the banner's bottom edge.
// Sharing `banner-inner` keeps the two heights identical even as the YAML
// adds or removes title / lunatech_since / contact lines.
#let banner-inner = grid(
  columns: (1fr, auto),
  column-gutter: 6mm,
  align: (left + top, right + top),

  // ── header left
  [
    #text(font: serif, size: 22pt, weight: 700, fill: white)[#cv-data.name]
    #v(1.5mm)
    #if opt(cv-data, "title") != "" [
      #text(size: 8.5pt, weight: 300, fill: white.transparentize(20%))[#cv-data.title]
    ]
    #if opt(cv-data, "lunatech_since") != "" [
      #v(0mm)
      #text(size: 8pt, weight: 300, style: "italic", fill: white.transparentize(40%))[
        #if opt(cv-data, "years_experience") != "" [#cv-data.years_experience years of experience #sym.dot.c ]
        Lunatech #sym.dash.em since #cv-data.lunatech_since
      ]
    ]
  ],

  // ── header right: Lunatech logo + (optional) contacts beneath it.
  // The logo is the brand anchor and stays whether or not the YAML
  // carries email / availability / location.
  [
    #set align(right)
    #image("/lunatech-logo-alone.png", height: 14mm)
    #set text(size: 7.5pt, weight: 300, fill: white.transparentize(15%))
    #if opt(cv-data, "email") != "" [
      #v(2mm)
      #text(fill: white.transparentize(40%))[✉] #h(1mm) #cv-data.email
    ]
    #if opt(cv-data, "availability") != "" [
      #linebreak()
      #text(fill: white.transparentize(40%))[◎] #h(1mm)
      #text(weight: 600, fill: white)[Available : #cv-data.availability]
    ]
    #if opt(cv-data, "location") != "" [
      #linebreak()
      #text(fill: white.transparentize(40%))[⌖] #h(1mm) #cv-data.location
    ]
  ],
)

// Visible banner — placed so it bleeds 16mm above the content area into
// the top margin (i.e., flush with the paper's top edge). `place` with
// `float: false` (default) doesn't take flow space, so we add a separate
// flow reservation below.
#place(top + left, dx: 0pt, dy: -16mm, block(
  fill: p.header-bg,
  width: 100%,
  inset: (left: 12.7mm, right: 12.7mm, top: 9mm, bottom: 7mm),
  banner-inner,
))

// In-flow reservation — same content laid out at the same width, but
// invisible. The visible banner spans paper y=0 to y=(16+C) where C is the
// content height. Of that, 16mm is bled into the top margin and (C) is in
// the content area. Reserving exactly C mm of flow space puts the next
// flow item flush against the visible banner's bottom edge. The visible
// banner's 7mm bottom inset is dark-fill *inside* the banner — not a gap
// below it — so we don't add it here.
#hide(block(
  width: 100%,
  inset: (left: 12.7mm, right: 12.7mm, top: 0pt, bottom: 0pt),
  banner-inner,
))

// ─────────── KEY ASSETS CAPSULE ───────────

#if opt-arr(cv-data, "key_assets").len() > 0 {
  let title = if opt(cv-data, "client_name") != "" {
    "Key Assets for " + cv-data.client_name
  } else { "Key Assets" }
  let asset-cell(a) = grid(
    columns: (4mm, 1fr),
    column-gutter: 1mm,
    align: (left + top, left + top),
    text(size: 7.5pt, fill: p.bullet)[◆],
    text(size: 9pt, weight: 500, fill: p.text)[#a],
  )
  block(
    fill: p.capsule-bg,
    width: 100%,
    inset: (left: 12.7mm, right: 12.7mm, top: 4mm, bottom: 4.5mm),
    [
      #text(size: 8pt, weight: 600, fill: p.accent, tracking: 0.8pt)[#upper(title)]
      #v(2mm)
      #grid(
        columns: (1fr, 1fr),
        column-gutter: 6mm,
        row-gutter: 2mm,
        ..cv-data.key_assets.map(asset-cell),
      )
    ],
  )
}

// ─────────── BODY HELPERS ───────────

#let section-title(name, bullet: "◆") = block(below: 2.5mm, sticky: true, [
  #grid(columns: (auto, 1fr), column-gutter: 2mm, align: (left + horizon, left + horizon),
    text(size: 8pt, weight: 600, fill: p.bullet)[#bullet],
    text(size: 7.5pt, weight: 600, fill: p.text, tracking: 0.6pt)[#upper(name)]
  )
  #v(1mm)
  #line(length: 100%, stroke: 0.6pt + p.accent)
])

#let tags(items) = if items.len() == 0 [] else {
  let parts = ()
  let first = true
  for t in items {
    if not first {
      parts.push(text(fill: p.bullet, weight: 600)[ #sym.dot.c ])
    }
    parts.push(text(weight: 600, size: 7.5pt, fill: p.text)[#t])
    first = false
  }
  parts.join()
}

// `breakable: true` — a single very long mission description (Daan's CVs
// have ~200-word ones) is taller than the remaining first-page space, and
// `breakable: false` would dump the whole entry onto page 2 leaving page
// one half-empty. Allow the description to flow across pages; the section
// title above it is `sticky: true` so headings still travel with content.
#let exp-block(exp) = block(below: 3mm, breakable: true, [
  #grid(
    columns: (10mm, 4mm, 1fr),
    column-gutter: 2mm,
    align: (center + top, center + top, left + top),

    text(size: 7pt, style: "italic", fill: p.muted)[#opt(exp, "period")],
    text(size: 8pt, fill: p.bullet)[◆],
    [
      #text(font: serif, size: 10pt, weight: 700)[
        #opt(exp, "company")
        #if opt(exp, "role") != "" [
          #text(weight: 400, fill: p.muted)[ #sym.dot.c ]
          #text(style: "italic", weight: 500, fill: p.muted)[#exp.role]
        ]
      ]
      #v(-1mm)
      #if opt(exp, "description") != "" [
        #text(size: 8pt, fill: p.body)[#exp.description]
      ]
      #if opt-arr(exp, "tags").len() > 0 [
        #v(0.5mm)
        #tags(opt-arr(exp, "tags"))
      ]
    ],
  )
])

#let block-entry(title-body, meta: "", desc: "", tag-arr: ()) = block(below: 2.5mm, breakable: true, [
  #text(font: serif, size: 9.5pt, weight: 700)[#title-body]
  #if meta != "" [
    #v(-1.2mm)
    #text(size: 7pt, fill: p.muted)[#meta]
  ]
  #if desc != "" [
    #v(-0.6mm)
    #text(size: 7.5pt, fill: p.body)[#desc]
  ]
  #if tag-arr.len() > 0 [
    #v(0.5mm)
    #tags(tag-arr)
  ]
])

#let project-block(proj) = {
  let title = if opt(proj, "subtitle") != "" {
    [
      #opt(proj, "name")
      #text(weight: 400, fill: p.muted)[ #sym.dash.em ]
      #text(style: "italic", weight: 500, fill: p.muted)[#proj.subtitle]
    ]
  } else { [#opt(proj, "name")] }
  block-entry(title,
    desc: opt(proj, "description"),
    tag-arr: opt-arr(proj, "tags"))
}

// `year` is sometimes a quoted string in the YAML and sometimes a bare
// integer (`2026` vs `"2026"`); coerce with `str(...)` so `parts.join`
// doesn't try to mix string and integer values.
#let cert-block(c) = {
  let parts = ()
  if opt(c, "subtitle") != "" { parts.push(str(c.subtitle)) }
  if opt(c, "issuer")   != "" { parts.push(str(c.issuer)) }
  if opt(c, "year")     != "" { parts.push(str(c.year)) }
  let meta = parts.join(" " + sym.dot.c + " ")
  block-entry([#opt(c, "name")], meta: meta)
}

#let edu-block(e) = {
  let title = if opt(e, "degree") != "" { e.degree } else { opt(e, "school") }
  let parts = ()
  if opt(e, "school") != "" and opt(e, "degree") != "" and e.school != title { parts.push(str(e.school)) }
  if opt(e, "year")   != "" { parts.push(str(e.year)) }
  let meta = parts.join(" " + sym.dot.c + " ")
  block-entry([#title], meta: meta)
}

#let skill-row(name, level) = {
  let pct = calc.max(0, calc.min(5, level)) / 5
  block(below: 1.6mm, grid(
    columns: (24mm, 1fr),
    column-gutter: 3mm,
    align: (left + horizon, left + horizon),
    text(size: 7.5pt, fill: p.text)[#name],
    box(width: 100%, height: 1.4mm, fill: p.bar-empty)[
      #place(left + horizon, rect(
        width: pct * 100%,
        height: 1.4mm,
        fill: p.bar-fill,
        stroke: none,
      ))
    ],
  ))
}

// ─────────── BODY ───────────

#grid(
  columns: (60%, 40%),
  column-gutter: 0pt,

  // ─── MAIN COLUMN ───
  block(
    inset: (left: 12.7mm, right: 5mm, top: 5mm, bottom: 0pt),
    {
      if opt(cv-data, "summary") != "" {
        section-title("Profile")
        text(font: serif, size: 9pt, fill: p.body)[#cv-data.summary]
        v(3mm)
      }
      if opt-arr(cv-data, "experiences").len() > 0 {
        section-title("Experience")
        for exp in cv-data.experiences { exp-block(exp) }
      }
    }
  ),

  // ─── SIDE COLUMN ───
  block(
    inset: (left: 5mm, right: 12.7mm, top: 5mm, bottom: 0pt),
    {
      if opt-arr(cv-data, "skills").len() > 0 {
        section-title("Skills")
        for (i, group) in opt-arr(cv-data, "skills").enumerate() {
          for item in opt-arr(group, "items") {
            let name = if type(item) == str { item } else { opt(item, "name") }
            let level = if type(item) == str { 3 } else { opt(item, "level", default: 3) }
            skill-row(name, level)
          }
          if i < opt-arr(cv-data, "skills").len() - 1 { v(2mm) }
        }
        v(3mm)
      }

      if opt-arr(cv-data, "projects").len() > 0 {
        section-title("Personal Project", bullet: "★")
        for proj in cv-data.projects { project-block(proj) }
        v(2mm)
      }

      if opt-arr(cv-data, "certifications").len() > 0 {
        section-title("Certifications", bullet: "★")
        for c in cv-data.certifications { cert-block(c) }
        v(2mm)
      }

      if opt-arr(cv-data, "education").len() > 0 {
        section-title("Education", bullet: "◇")
        for e in cv-data.education { edu-block(e) }
        v(2mm)
      }

      if opt-arr(cv-data, "languages").len() > 0 {
        section-title("Languages")
        for l in cv-data.languages [
          #grid(columns: (1fr, auto), align: (left + horizon, right + horizon),
            text(size: 7.5pt, fill: p.text)[#l.language],
            text(size: 7pt, style: "italic", fill: p.muted)[#l.level]
          )
          #v(-0.5mm)
        ]
      }
    }
  )
)

