// Renders a `Review` to a Lunatech-branded PDF via Typst.
//
// The cv-reviewer skill returns the report body as Markdown, so we convert it
// to Typst syntax with `pulldown-cmark` (events stream → matching Typst
// markup) and inject the result into a small inline template that carries
// the score / verdict / date header. We deliberately do NOT reuse the CV
// template — the layout is different (paginated long-form text vs. fixed
// single-page layout), and tying them together would couple two unrelated
// designs.

use crate::cv_reviewer::Review;
use crate::pdf;
use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::fmt::Write;

pub fn render(review: &Review, cv_name: Option<&str>) -> Result<Vec<u8>> {
    let body_typst = markdown_to_typst(&review.report_markdown);
    let source = build_template(review, cv_name, &body_typst);
    if std::env::var("CV_DEBUG_TYPST").is_ok() {
        let _ = std::fs::write("/tmp/cv-builder-review-debug.typ", &source);
    }
    pdf::compile(source)
}

fn build_template(review: &Review, cv_name: Option<&str>, body: &str) -> String {
    // Score → color matches the frontend modal pill (low / mid / high).
    let (score_color, _score_label) = match review.overall_score {
        8..=10 => ("rgb(\"#10b981\")", "high"),
        5..=7 => ("rgb(\"#f59e0b\")", "mid"),
        _ => ("rgb(\"#ef4444\")", "low"),
    };
    let verdict = match review.verdict.as_str() {
        "client_ready" => "Client ready",
        "minor_improvements" => "Minor improvements",
        "major_rework" => "Major rework",
        other => other,
    };
    let title = match cv_name {
        Some(name) if !name.trim().is_empty() => format!("CV Review — {}", name.trim()),
        _ => "CV Review".to_string(),
    };

    let title_content = typst_content(&title);
    let verdict_upper = typst_content(&verdict.to_uppercase());

    let mut s = String::new();
    writeln!(s, "#set document(title: {})", typst_string(&title)).unwrap();
    s.push_str(r##"#set page(
  paper: "a4",
  margin: (top: 18mm, bottom: 18mm, x: 16mm),
  footer: align(center, text(size: 7pt, fill: rgb("#6b6b6b"), tracking: 0.4pt)[
    Lunatech France #h(2mm) | #h(2mm) +33 1 82 88 56 64 #h(2mm) | #h(2mm) info\@lunatech.fr
  ]),
)
#set text(font: ("Poppins", "Inter", "Helvetica"), size: 10pt, fill: rgb("#1a1a1a"))
#set par(leading: 0.65em, justify: false)
#show heading.where(level: 1): set text(font: ("EB Garamond", "Georgia", "Times"), size: 22pt, weight: 700)
#show heading.where(level: 2): set text(size: 13pt, weight: 600, fill: rgb("#7c1818"), tracking: 0.3pt)
#show heading.where(level: 3): set text(size: 11pt, weight: 600)
#show table.cell.where(y: 0): strong
#set table(stroke: (_, _) => 0.4pt + rgb("#d8d4d0"), inset: 5pt)
"##);

    // Header banner — score on the right, title + verdict on the left.
    write!(
        s,
        r##"#block(
  fill: rgb("#1a1a1a"),
  width: 100% + 32mm,
  inset: (x: 16mm, y: 7mm),
  outset: (x: 16mm),
  [
    #grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      column-gutter: 6mm,
      [
        #text(font: ("EB Garamond", "Georgia", "Times"), size: 22pt, weight: 700, fill: white)[{title}]
        #v(0.5mm)
        #text(size: 8pt, weight: 300, fill: white.transparentize(20%), tracking: 0.6pt)[{verdict}]
      ],
      [
        #box(fill: {score_color}, inset: (x: 5mm, y: 3mm), radius: 2mm)[
          #text(size: 18pt, weight: 700, fill: white)[{score} / 10]
        ]
      ],
    )
  ],
)
#v(6mm)
"##,
        title = title_content,
        verdict = verdict_upper,
        score_color = score_color,
        score = review.overall_score,
    )
    .unwrap();

    s.push_str(body);
    s
}

/// Escape a Rust string into a Typst string literal: `"..."`.
fn typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Escape a Rust string for use as Typst content (inside `[...]`). Escapes
/// the characters that have syntactic meaning so they render as literals.
fn typst_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '#' | '*' | '_' | '[' | ']' | '<' | '>' | '@' | '$' | '~' | '`' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Stream-converts CommonMark + GFM tables to Typst markup. Handles only the
/// constructs the cv-reviewer skill actually emits: headings, paragraphs,
/// lists (ordered + unordered, nested), bold / italic / inline code, code
/// blocks, tables, links, hard / soft breaks, and horizontal rules.
fn markdown_to_typst(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    // Output stack: cell capture pushes a new buffer, end-of-cell pops.
    let mut targets: Vec<String> = vec![String::new()];
    let mut ordered: Vec<bool> = Vec::new(); // one entry per open list
    let mut in_table_first_row = false;
    let mut table_cols: usize = 0;
    let mut table_cells: Vec<String> = Vec::new();
    let mut in_code_block: usize = 0; // depth — text inside is emitted raw

    fn cur(targets: &mut [String]) -> &mut String {
        targets.last_mut().expect("output target stack underflow")
    }

    for event in Parser::new_ext(md, opts) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                cur(&mut targets).push('\n');
                cur(&mut targets).push_str(&"=".repeat(n));
                cur(&mut targets).push(' ');
            }
            Event::End(TagEnd::Heading(_)) => {
                cur(&mut targets).push_str("\n\n");
            }

            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                cur(&mut targets).push_str("\n\n");
            }

            Event::Start(Tag::Strong) => cur(&mut targets).push('*'),
            Event::End(TagEnd::Strong) => cur(&mut targets).push('*'),
            Event::Start(Tag::Emphasis) => cur(&mut targets).push('_'),
            Event::End(TagEnd::Emphasis) => cur(&mut targets).push('_'),
            Event::Start(Tag::Strikethrough) => cur(&mut targets).push_str("#strike["),
            Event::End(TagEnd::Strikethrough) => cur(&mut targets).push(']'),

            Event::Code(code) => {
                cur(&mut targets).push_str("#raw(");
                cur(&mut targets).push_str(&typst_string(&code));
                cur(&mut targets).push(')');
            }
            // Code blocks: capture into a fresh target, then wrap as a Typst
            // string literal under #raw so the contents bypass the Typst
            // syntax (otherwise stars / brackets / backticks inside code
            // would get the body-text escape treatment and look wrong).
            Event::Start(Tag::CodeBlock(_)) => {
                targets.push(String::new());
                in_code_block += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = in_code_block.saturating_sub(1);
                let captured = targets.pop().expect("code block underflow");
                cur(&mut targets).push_str("\n#raw(block: true, ");
                cur(&mut targets).push_str(&typst_string(&captured));
                cur(&mut targets).push_str(")\n\n");
            }

            Event::Start(Tag::List(start)) => {
                ordered.push(start.is_some());
            }
            Event::End(TagEnd::List(_)) => {
                ordered.pop();
                if ordered.is_empty() {
                    cur(&mut targets).push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                let depth = ordered.len().saturating_sub(1);
                let marker = if *ordered.last().unwrap_or(&false) { "+" } else { "-" };
                cur(&mut targets).push('\n');
                cur(&mut targets).push_str(&"  ".repeat(depth));
                cur(&mut targets).push_str(marker);
                cur(&mut targets).push(' ');
            }
            Event::End(TagEnd::Item) => {}

            Event::Start(Tag::Table(_)) => {
                in_table_first_row = true;
                table_cols = 0;
                table_cells.clear();
            }
            Event::End(TagEnd::Table) => {
                let mut t = String::new();
                let cols = table_cols.max(1);
                writeln!(t, "\n#table(columns: {},", cols).unwrap();
                for cell in &table_cells {
                    write!(t, "  [{}],\n", cell.trim()).unwrap();
                }
                t.push_str(")\n\n");
                cur(&mut targets).push_str(&t);
                table_cells.clear();
                table_cols = 0;
            }
            Event::Start(Tag::TableHead) => {}
            Event::End(TagEnd::TableHead) => {
                in_table_first_row = false;
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {
                targets.push(String::new());
            }
            Event::End(TagEnd::TableCell) => {
                let cell = targets.pop().expect("table cell pop underflow");
                table_cells.push(cell);
                if in_table_first_row {
                    table_cols += 1;
                }
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                cur(&mut targets).push_str("#link(");
                cur(&mut targets).push_str(&typst_string(&dest_url));
                cur(&mut targets).push_str(")[");
            }
            Event::End(TagEnd::Link) => cur(&mut targets).push(']'),

            Event::Start(Tag::BlockQuote(_)) => cur(&mut targets).push_str("\n#quote(block: true)["),
            Event::End(TagEnd::BlockQuote(_)) => cur(&mut targets).push_str("]\n\n"),

            Event::Rule => cur(&mut targets).push_str("\n#line(length: 100%)\n\n"),

            Event::SoftBreak => cur(&mut targets).push(' '),
            Event::HardBreak => cur(&mut targets).push_str(" \\\n"),

            Event::Text(text) => {
                if in_code_block > 0 {
                    cur(&mut targets).push_str(&text);
                } else {
                    cur(&mut targets).push_str(&typst_content(&text));
                }
            }

            // Ignore HTML, footnotes, task list markers, etc. — not produced
            // by the cv-reviewer skill.
            _ => {}
        }
    }

    targets.into_iter().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(md: &str) -> String {
        markdown_to_typst(md)
    }

    #[test]
    fn headings_use_equals() {
        let s = convert("# H1\n\n## H2\n\n### H3\n");
        assert!(s.contains("= H1"));
        assert!(s.contains("== H2"));
        assert!(s.contains("=== H3"));
    }

    #[test]
    fn bold_and_italic_round_trip() {
        let s = convert("This is **bold** and *italic*.");
        assert!(s.contains("*bold*"));
        assert!(s.contains("_italic_"));
    }

    #[test]
    fn unordered_lists_use_dash() {
        let s = convert("- one\n- two\n- three\n");
        assert!(s.contains("- one"));
        assert!(s.contains("- two"));
        assert!(s.contains("- three"));
    }

    #[test]
    fn ordered_lists_use_plus() {
        let s = convert("1. first\n2. second\n");
        assert!(s.contains("+ first"));
        assert!(s.contains("+ second"));
    }

    #[test]
    fn special_chars_are_escaped() {
        let s = convert("Plain text with [brackets] and #hash and *literal star*.");
        // The literal-star markdown becomes Typst bold; brackets/hash are
        // escaped because they're plain text, not Typst syntax.
        assert!(s.contains("\\[brackets\\]"));
        assert!(s.contains("\\#hash"));
    }

    #[test]
    fn table_emits_typst_table() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        let s = convert(md);
        assert!(s.contains("#table(columns: 2"));
        assert!(s.contains("[a]"));
        assert!(s.contains("[b]"));
        assert!(s.contains("[1]"));
        assert!(s.contains("[4]"));
    }

    #[test]
    fn render_produces_pdf_bytes() {
        let r = Review {
            overall_score: 7,
            verdict: "minor_improvements".into(),
            language: "en".into(),
            report_markdown: "# Overall\n\nGood enough.\n\n## Per-Project\n\n- One\n- Two\n".into(),
            improved_yaml: "".into(),
        };
        let bytes = render(&r, Some("Alice")).expect("render review pdf");
        assert!(bytes.starts_with(b"%PDF-"), "expected PDF magic bytes");
    }
}
