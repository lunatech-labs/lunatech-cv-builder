// Lunatech CV — Typst template, layout cv-example.pdf.
// `cv-data` is injected as a #let by the Rust side before this file runs.

#let theme = if "theme" in cv-data { cv-data.theme } else { "lunatech" }

#let palette-lunatech = (
  accent:     rgb("#7c1818"),
  header-bg:  rgb("#1a1a1a"),
  bar-fill:   rgb("#7c1818"),
  bar-empty:  rgb("#f0d8d8"),
  bullet:     rgb("#7c1818"),
  text:       rgb("#1a1a1a"),
  body:       rgb("#2a2a2a"),
  muted:      rgb("#6b6b6b"),
  border:     rgb("#d8d4d0"),
)

#let palette-luxe = (
  accent:     rgb("#0B0B0B"),
  header-bg:  rgb("#0B0B0B"),
  bar-fill:   rgb("#c8a24a"),
  bar-empty:  rgb("#e8e2d4"),
  bullet:     rgb("#c8a24a"),
  text:       rgb("#1a1a1a"),
  body:       rgb("#2a2a2a"),
  muted:      rgb("#6b6b6b"),
  border:     rgb("#e8e2d4"),
)

#let palette-cosmic = (
  accent:     rgb("#0a1e50"),
  header-bg:  rgb("#0a1e50"),
  bar-fill:   rgb("#DB2777"),
  bar-empty:  rgb("#f0d8e8"),
  bullet:     rgb("#DB2777"),
  text:       rgb("#1a1a1a"),
  body:       rgb("#2a2a2a"),
  muted:      rgb("#6b6b6b"),
  border:     rgb("#dde0e8"),
)

#let palette-opera = (
  accent:     rgb("#9b0f17"),
  header-bg:  rgb("#9b0f17"),
  bar-fill:   rgb("#ea212e"),
  bar-empty:  rgb("#f5dede"),
  bullet:     rgb("#ea212e"),
  text:       rgb("#1a1a1a"),
  body:       rgb("#2a2a2a"),
  muted:      rgb("#6b6b6b"),
  border:     rgb("#e8d8d8"),
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
#let serif = ("EB Garamond", "Garamond", "Times New Roman", "Times", "DejaVu Serif", "Liberation Serif", ..symbols)
#let sans  = ("Poppins", "Inter", "Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans", "Liberation Sans", ..symbols)

#set document(title: cv-data.name + " — CV")
#set page(
  paper: "a4",
  margin: (top: 0pt, bottom: 16mm, x: 0pt),
  footer-descent: 6mm,
  footer: align(center, block(width: 100% - 25.4mm)[
    #line(length: 100%, stroke: 0.4pt + p.border)
    #v(1mm)
    #align(center, text(size: 6.5pt, fill: p.muted, tracking: 0.5pt)[
      Lunatech France #h(2mm) | #h(2mm) 3 rue de la Galmy 77700 Chessy, France #h(2mm) | #h(2mm) +33 1 82 88 56 64 #h(2mm) | #h(2mm) info\@lunatech.fr
    ])
  ]),
)
#set text(font: sans, size: 9pt, fill: p.text, hyphenate: false)
#set par(leading: 0.55em, justify: false)

#let opt(d, k, default: "") = if k in d { d.at(k) } else { default }
#let opt-arr(d, k) = if k in d and d.at(k) != none { d.at(k) } else { () }

// ─────────── HEADER ───────────

#block(
  fill: p.header-bg,
  width: 100%,
  inset: (left: 12.7mm, right: 12.7mm, top: 9mm, bottom: 7mm),
  [
    #grid(
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

      // ── header right (contacts)
      [
        #set align(right)
        #set text(size: 7.5pt, weight: 300, fill: white.transparentize(15%))
        #let email = opt(cv-data, "email", default: "info@lunatech.fr")
        #text(fill: white.transparentize(40%))[✉] #h(1mm) #email
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
  ],
)

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

#let exp-block(exp) = block(below: 3mm, breakable: false, [
  #grid(
    columns: (10mm, 4mm, 1fr),
    column-gutter: 2mm,
    align: (center + top, center + top, left + top),

    text(size: 6.5pt, style: "italic", fill: p.muted)[#opt(exp, "period")],
    text(size: 8pt, fill: p.bullet)[◆],
    [
      #text(font: serif, size: 10pt, weight: 700)[
        #opt(exp, "company")
        #if opt(exp, "role") != "" [
          #text(weight: 400, fill: p.muted)[ #sym.dot.c ]
          #text(style: "italic", weight: 500)[#exp.role]
        ]
      ]
      #v(-1mm)
      #if opt(exp, "description") != "" [
        #text(font: serif, size: 8.5pt, style: "italic", fill: p.body)[#exp.description]
      ]
      #if opt-arr(exp, "tags").len() > 0 [
        #v(0.5mm)
        #tags(opt-arr(exp, "tags"))
      ]
    ],
  )
])

#let block-entry(title-body, meta: "", desc: "", tag-arr: ()) = block(below: 2.5mm, breakable: false, [
  #text(font: serif, size: 9.5pt, weight: 700)[#title-body]
  #if meta != "" [
    #v(-1.2mm)
    #text(font: serif, size: 8pt, style: "italic", fill: p.body)[#meta]
  ]
  #if desc != "" [
    #v(-0.6mm)
    #text(font: serif, size: 8.5pt, style: "italic", fill: p.body)[#desc]
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
      #text(style: "italic", weight: 500)[#proj.subtitle]
    ]
  } else { [#opt(proj, "name")] }
  block-entry(title,
    desc: opt(proj, "description"),
    tag-arr: opt-arr(proj, "tags"))
}

#let cert-block(c) = {
  let parts = ()
  if opt(c, "subtitle") != "" { parts.push(c.subtitle) }
  if opt(c, "issuer")   != "" { parts.push(c.issuer) }
  if opt(c, "year")     != "" { parts.push(c.year) }
  let meta = parts.join(" " + sym.dot.c + " ")
  block-entry([#opt(c, "name")], meta: meta)
}

#let edu-block(e) = {
  let title = if opt(e, "degree") != "" { e.degree } else { opt(e, "school") }
  let parts = ()
  if opt(e, "school") != "" and opt(e, "degree") != "" and e.school != title { parts.push(e.school) }
  if opt(e, "year")   != "" { parts.push(e.year) }
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
        text(font: serif, size: 9pt, style: "italic", fill: p.body)[#cv-data.summary]
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

      if opt-arr(cv-data, "key_assets").len() > 0 {
        let title = if opt(cv-data, "client_name") != "" {
          "Key Assets for " + cv-data.client_name
        } else { "Key Assets" }
        section-title(title, bullet: "▸")
        for a in cv-data.key_assets [
          #grid(columns: (4mm, 1fr), column-gutter: 1mm, align: (left + top, left + top),
            text(size: 6pt, fill: p.bullet)[◆],
            text(size: 7.5pt, fill: p.body)[#a],
          )
          #v(0.5mm)
        ]
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

