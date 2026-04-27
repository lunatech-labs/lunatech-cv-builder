// Lunatech CV — Typst version mirroring the HTML template.
// `cv-data` is injected as a #let by the Rust side before this file runs.

#let theme = if "theme" in cv-data { cv-data.theme } else { "cosmic" }

#let palette-cosmic = (
  accent: rgb("#0a1e50"),
  accent-light: rgb("#e8edf5"),
  shadow: rgb("#DB2777"),
  key-bg: rgb("#0a1e50"),
  bar-fill: rgb("#DB2777"),
  bar-empty: rgb("#f0d8e8"),
  border: rgb("#dde0e8"),
  tag-bg: rgb("#fce7f3"),
  tag-text: rgb("#9d174d"),
  text: rgb("#1c1c1e"),
  body: rgb("#282828"),
  muted: rgb("#595959"),
)

#let palette-luxe = (
  accent: rgb("#0B0B0B"),
  accent-light: rgb("#f5f0e8"),
  shadow: rgb("#c8a24a"),
  key-bg: rgb("#0B0B0B"),
  bar-fill: rgb("#c8a24a"),
  bar-empty: rgb("#e8e2d4"),
  border: rgb("#e8e2d4"),
  tag-bg: rgb("#f5f0e8"),
  tag-text: rgb("#0B0B0B"),
  text: rgb("#1c1c1e"),
  body: rgb("#282828"),
  muted: rgb("#595959"),
)

#let palette-opera = (
  accent: rgb("#9b0f17"),
  accent-light: rgb("#fdf2f2"),
  shadow: rgb("#ea212e"),
  key-bg: rgb("#9b0f17"),
  bar-fill: rgb("#ea212e"),
  bar-empty: rgb("#f5dede"),
  border: rgb("#e8d8d8"),
  tag-bg: rgb("#f5dede"),
  tag-text: rgb("#7a0c13"),
  text: rgb("#1c1c1e"),
  body: rgb("#282828"),
  muted: rgb("#595959"),
)

#let palettes = (cosmic: palette-cosmic, luxe: palette-luxe, opera: palette-opera)
#let p = if theme in palettes { palettes.at(theme) } else { palette-cosmic }

#set document(title: cv-data.name + " — CV")
#set page(paper: "a4", margin: 0pt)
#set text(
  font: ("Poppins", "Inter", "Helvetica Neue", "Helvetica", "Arial"),
  size: 9pt,
  fill: p.text,
  hyphenate: false,
)
#set par(leading: 0.55em, justify: false)

#let opt(d, k, default: "") = if k in d { d.at(k) } else { default }
#let opt-arr(d, k) = if k in d and d.at(k) != none { d.at(k) } else { () }

// ─────────── HEADER ───────────
// SVG polygons in source: shadow (under) and accent (top)
// Source coords (viewBox 794x155): scale to A4 width 210mm.
// Shadow: 0,20  794,20  794,135  0,175  → outside page bottom is fine, gets clipped
// Accent: 0,0   794,0   794,105  0,155

#let mm-x(px) = px * 210mm / 794
#let mm-y(px) = px * 210mm / 794   // same scale (A4 is 210x297; we use uniform horizontal scale for header band)

#place(top + left, dx: 0pt, dy: 0pt, polygon(
  fill: p.shadow,
  stroke: none,
  (0pt, mm-y(20)),
  (mm-x(794), mm-y(20)),
  (mm-x(794), mm-y(135)),
  (0pt, mm-y(175)),
))

#place(top + left, dx: 0pt, dy: 0pt, polygon(
  fill: p.accent,
  stroke: none,
  (0pt, 0pt),
  (mm-x(794), 0pt),
  (mm-x(794), mm-y(105)),
  (0pt, mm-y(155)),
))

// Header text (name / title / contacts)
#place(top + left, dx: 12.7mm, dy: 8mm, block(width: 140mm, [
  #set text(fill: white)
  #text(size: 22pt, weight: 600, tracking: -0.01em)[#cv-data.name]
  #v(0.3mm)
  #text(size: 8pt, weight: 300, tracking: 1.2pt)[#upper(cv-data.title)]
  #v(2mm)
  #text(size: 8pt, weight: 300)[
    #text(fill: white.transparentize(35%))[✉] #h(0.6mm) info\@lunatech.com
    #h(5mm)
    #text(fill: white.transparentize(35%))[◈] #h(0.6mm) Lunatech since #opt(cv-data, "lunatech_since")
  ]
]))

// ─────────── BODY ───────────

#v(48mm)  // push past the header

#let section-title(name) = block(below: 3mm, [
  #text(size: 7pt, weight: 600, fill: p.muted, tracking: 1.4pt)[#upper(name)]
  #v(1.5mm)
  #line(length: 100%, stroke: 0.5pt + p.border)
])

#let tag(t) = box(
  fill: p.tag-bg,
  inset: (x: 4pt, y: 1pt),
  outset: 0pt,
  radius: 1.5pt,
)[
  #text(size: 7pt, weight: 500, fill: p.tag-text)[#t]
]

#let tags(items) = if items.len() == 0 [] else {
  // wrap with a small gap between each tag
  items.map(t => tag(t)).join(h(2pt))
}

#let exp-block(exp) = block(below: 4mm, [
  #grid(columns: (1fr, auto), align: (left + top, right + top),
    text(size: 10pt, weight: 600)[#opt(exp, "company")],
    text(size: 7.5pt, weight: 300, fill: p.muted)[#opt(exp, "period")]
  )
  #v(-1.5mm)
  #text(size: 8.5pt, fill: p.accent)[#opt(exp, "role")]
  #v(0.5mm)
  #text(size: 8pt, weight: 300, fill: p.body)[#opt(exp, "description")]
  #if opt-arr(exp, "tags").len() > 0 [
    #v(1mm)
    #tags(opt-arr(exp, "tags"))
  ]
])

#let project-block(proj) = block(below: 3mm, [
  #grid(columns: (1fr, auto), align: (left + top, right + top),
    text(size: 9.5pt, weight: 600)[#opt(proj, "name")],
    if opt(proj, "link") != "" [#text(size: 7.5pt, weight: 300, fill: p.accent)[#proj.link]] else []
  )
  #v(-1mm)
  #text(size: 8pt, weight: 300, fill: p.body)[#opt(proj, "description")]
  #if opt-arr(proj, "tags").len() > 0 [
    #v(1mm)
    #tags(opt-arr(proj, "tags"))
  ]
])

#let cert-block(c, label-key) = block(below: 2mm, [
  #grid(columns: (auto, 1fr), column-gutter: 2mm, align: (left + top, left + top),
    box(fill: p.tag-bg, inset: (x: 4pt, y: 2pt), radius: 1.5pt,
      text(size: 7pt, weight: 600, fill: p.tag-text)[#opt(c, "year")]),
    [
      #text(size: 8pt, weight: 400)[#opt(c, label-key)]
      #v(-1mm)
      #text(size: 7.5pt, weight: 300, fill: p.body)[#opt(c, if label-key == "school" { "degree" } else { "issuer" })]
    ]
  )
])

#let skill-bar-row(name, level) = {
  let segs = ()
  for i in range(5) {
    segs.push(rect(
      width: 100%,
      height: 3pt,
      fill: if i < level { p.bar-fill } else { p.bar-empty },
      radius: 1pt,
      stroke: none,
    ))
  }
  block(below: 1.6mm, grid(
    columns: (22mm, 1fr),
    column-gutter: 2mm,
    align: (left + horizon, left + horizon),
    text(size: 7.5pt)[#name],
    grid(columns: (1fr,) * 5, column-gutter: 1.5pt, ..segs)
  ))
}

// Two-column body
#grid(
  columns: (62%, 38%),
  column-gutter: 0pt,

  // ─── MAIN COLUMN ───
  block(
    inset: (left: 12.7mm, right: 5.8mm, top: 0pt, bottom: 0pt),
    stroke: (right: 0.5pt + p.border),
    {
      if opt(cv-data, "summary") != "" [
        #text(size: 8.5pt, weight: 300, style: "italic", fill: p.body)[#cv-data.summary]
        #v(3mm)
      ]
      if opt-arr(cv-data, "experiences").len() > 0 [
        #section-title("Professional Experience")
        #for exp in cv-data.experiences { exp-block(exp) }
      ]
      if opt-arr(cv-data, "projects").len() > 0 [
        #section-title("Personal Projects & Interests")
        #for proj in cv-data.projects { project-block(proj) }
      ]
    }
  ),

  // ─── SIDE COLUMN ───
  block(
    inset: (left: 4.7mm, right: 6.4mm, top: 0pt, bottom: 0pt),
    {
      // Spacer where the photo would sit
      v(22mm)

      // Key assets capsule
      if opt-arr(cv-data, "key_assets").len() > 0 {
        block(
          fill: p.key-bg,
          radius: 2.5pt,
          inset: (x: 9pt, y: 8pt),
          width: 100%,
          below: 4mm,
          [
            #text(size: 6.5pt, weight: 600, fill: white.transparentize(50%), tracking: 1.4pt)[#upper("Key Assets for")]
            #v(-0.5mm)
            #text(size: 8.5pt, weight: 600, fill: white)[#opt(cv-data, "client_name", default: "Client")]
            #v(2mm)
            #for a in cv-data.key_assets [
              #grid(columns: (3mm, 1fr), align: (left + top, left + top),
                text(size: 7pt, fill: white.transparentize(65%))[—],
                text(size: 7pt, fill: white.transparentize(15%))[#a],
              )
              #v(-0.5mm)
            ]
          ]
        )
      }

      if opt-arr(cv-data, "skills").len() > 0 {
        section-title("Skills")
        for (i, group) in opt-arr(cv-data, "skills").enumerate() {
          for item in opt-arr(group, "items") {
            let name = if type(item) == "string" { item } else { opt(item, "name") }
            let level = if type(item) == "string" { 3 } else { opt(item, "level", default: 3) }
            skill-bar-row(name, level)
          }
          if i < opt-arr(cv-data, "skills").len() - 1 { v(2mm) }
        }
        v(2mm)
      }

      let edus = opt-arr(cv-data, "education")
      let certs = opt-arr(cv-data, "certifications")
      if edus.len() > 0 or certs.len() > 0 {
        section-title("Education & Certifications")
        for e in edus { cert-block(e, "school") }
        for c in certs { cert-block(c, "name") }
        v(2mm)
      }

      let langs = opt-arr(cv-data, "languages")
      if langs.len() > 0 {
        section-title("Languages")
        for l in langs [
          #grid(columns: (1fr, auto), align: (left + horizon, right + horizon),
            text(size: 8pt, fill: p.body)[#l.language],
            text(size: 7.5pt, weight: 300, fill: p.body)[#l.level]
          )
          #v(-0.5mm)
        ]
      }
    }
  )
)

// ─────────── FOOTER ───────────
#place(bottom + center, dy: -6mm, block(width: 100% - 21.2mm)[
  #line(length: 100%, stroke: 0.4pt + p.border)
  #v(1mm)
  #align(center, text(size: 6.5pt, fill: p.muted, tracking: 0.5pt)[
    Lunatech France #h(2mm) | #h(2mm) 3 rue de la Galmy 77700 Chessy, France #h(2mm) | #h(2mm) +33 1 82 88 56 64 #h(2mm) | #h(2mm) info\@lunatech.fr
  ])
])
