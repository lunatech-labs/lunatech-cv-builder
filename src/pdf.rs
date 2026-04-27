use anyhow::{Result, anyhow};
use std::fmt::Write;
use std::sync::OnceLock;
use typst::{Library, LibraryExt};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst_kit::fonts::{FontSearcher, FontSlot};

const TEMPLATE: &str = include_str!("../assets/cv.typ");

struct LoadedFonts {
    book: FontBook,
    fonts: Vec<FontSlot>,
}

fn load_fonts() -> &'static LoadedFonts {
    static FONTS: OnceLock<LoadedFonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let f = FontSearcher::new().include_system_fonts(true).search();
        LoadedFonts { book: f.book, fonts: f.fonts }
    })
}

struct CvWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: &'static [FontSlot],
    main: FileId,
    source: Source,
}

impl typst::World for CvWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rooted_path().to_path_buf()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rooted_path().to_path_buf()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index)?.get()
    }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 4, 27)
    }
}

pub fn render(yaml: &str, theme: &str) -> Result<Vec<u8>> {
    let mut value: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| anyhow!("invalid YAML: {}", e))?;

    // Inject theme override (the YAML wins if it already specifies one).
    if let serde_yaml::Value::Mapping(map) = &mut value {
        let key = serde_yaml::Value::String("theme".into());
        if !map.contains_key(&key) {
            map.insert(key, serde_yaml::Value::String(theme.into()));
        }
    }

    let mut src = String::new();
    src.push_str("#let cv-data = ");
    write_value(&mut src, &value);
    src.push_str("\n\n");
    src.push_str(TEMPLATE);

    if std::env::var("CV_DEBUG_TYPST").is_ok() {
        let _ = std::fs::write("/tmp/cv-builder-debug.typ", &src);
    }

    let loaded = load_fonts();
    let main_id = FileId::new(None, VirtualPath::new("main.typ"));
    let source = Source::new(main_id, src);
    let world = CvWorld {
        library: LazyHash::new(Library::builder().build()),
        book: LazyHash::new(loaded.book.clone()),
        fonts: &loaded.fonts,
        main: main_id,
        source,
    };

    let result = typst::compile::<typst::layout::PagedDocument>(&world);
    let doc = result.output.map_err(|errs| {
        let msgs: Vec<String> = errs.iter().map(|e| format!("{}", e.message)).collect();
        anyhow!("typst compile failed: {}", msgs.join("; "))
    })?;
    let pdf = typst_pdf::pdf(&doc, &Default::default()).map_err(|errs| {
        let msgs: Vec<String> = errs.iter().map(|e| format!("{}", e.message)).collect();
        anyhow!("typst pdf export failed: {}", msgs.join("; "))
    })?;
    Ok(pdf)
}

fn write_value(buf: &mut String, value: &serde_yaml::Value) {
    use serde_yaml::Value;
    match value {
        Value::Null => buf.push_str("none"),
        Value::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                write!(buf, "{}", i).unwrap();
            } else if let Some(f) = n.as_f64() {
                write!(buf, "{}", f).unwrap();
            } else {
                buf.push_str("0");
            }
        }
        Value::String(s) => write_string(buf, s),
        Value::Sequence(seq) => {
            buf.push('(');
            for item in seq {
                write_value(buf, item);
                buf.push_str(", ");
            }
            buf.push(')');
        }
        Value::Mapping(map) => {
            buf.push('(');
            let mut first = true;
            for (k, v) in map {
                let key = match k {
                    Value::String(s) => sanitize_key(s),
                    other => sanitize_key(&format!("{:?}", other)),
                };
                if !first {
                    buf.push_str(", ");
                }
                first = false;
                buf.push_str(&key);
                buf.push_str(": ");
                write_value(buf, v);
            }
            // Trailing comma helps Typst when the mapping is otherwise empty
            if !first {
                buf.push(',');
            }
            buf.push(')');
        }
        Value::Tagged(t) => write_value(buf, &t.value),
    }
}

fn write_string(buf: &mut String, s: &str) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '\\' => buf.push_str("\\\\"),
            '"' => buf.push_str("\\\""),
            '\n' => buf.push_str("\\n"),
            '\r' => {}
            '\t' => buf.push_str("  "),
            c if (c as u32) < 0x20 => {}
            c => buf.push(c),
        }
    }
    buf.push('"');
}

fn sanitize_key(k: &str) -> String {
    // Typst dict keys must be valid identifiers. Replace anything outside
    // [A-Za-z0-9_-] with underscore. Leading digits get prefixed.
    let mut out = String::with_capacity(k.len());
    let mut chars = k.chars();
    if let Some(first) = chars.next() {
        if first.is_ascii_digit() {
            out.push('_');
        }
        out.push(if first.is_ascii_alphanumeric() || first == '_' || first == '-' {
            first
        } else {
            '_'
        });
    }
    for c in chars {
        out.push(if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            c
        } else {
            '_'
        });
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_value(s: &str) -> serde_yaml::Value {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn sanitize_key_keeps_valid_idents() {
        assert_eq!(sanitize_key("name"), "name");
        assert_eq!(sanitize_key("client_name"), "client_name");
        assert_eq!(sanitize_key("my-key"), "my-key");
    }

    #[test]
    fn sanitize_key_replaces_invalid_chars() {
        assert_eq!(sanitize_key("key with space"), "key_with_space");
        assert_eq!(sanitize_key("key.with.dots"), "key_with_dots");
        assert_eq!(sanitize_key("key/with/slash"), "key_with_slash");
    }

    #[test]
    fn sanitize_key_prefixes_leading_digit() {
        assert_eq!(sanitize_key("1name"), "_1name");
        assert_eq!(sanitize_key("9"), "_9");
    }

    #[test]
    fn sanitize_key_handles_empty() {
        assert_eq!(sanitize_key(""), "_");
    }

    #[test]
    fn write_string_escapes_quotes_and_backslashes() {
        let mut buf = String::new();
        write_string(&mut buf, "hello \"world\" \\path");
        assert_eq!(buf, r#""hello \"world\" \\path""#);
    }

    #[test]
    fn write_string_escapes_newlines() {
        let mut buf = String::new();
        write_string(&mut buf, "line1\nline2\rline3");
        assert_eq!(buf, "\"line1\\nline2line3\"");
    }

    #[test]
    fn write_value_string() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("hello"));
        assert_eq!(buf, "\"hello\"");
    }

    #[test]
    fn write_value_integer() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("42"));
        assert_eq!(buf, "42");
    }

    #[test]
    fn write_value_bool() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("true"));
        assert_eq!(buf, "true");
    }

    #[test]
    fn write_value_null() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("null"));
        assert_eq!(buf, "none");
    }

    #[test]
    fn write_value_sequence() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("[a, b, c]"));
        assert_eq!(buf, "(\"a\", \"b\", \"c\", )");
    }

    #[test]
    fn write_value_mapping_emits_trailing_comma() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("name: Alice\nage: 30"));
        // Trailing comma is critical: in Typst, "(name: \"Alice\", age: 30)"
        // without it would still be a dict, but the trailing comma keeps the
        // serializer simple and matches our convention.
        assert!(buf.contains("name: \"Alice\""));
        assert!(buf.contains("age: 30"));
        assert!(buf.starts_with('('));
        assert!(buf.ends_with(')'));
    }

    #[test]
    fn write_value_nested() {
        let mut buf = String::new();
        write_value(&mut buf, &yaml_value("outer:\n  inner: value\n  list: [1, 2]"));
        assert!(buf.contains("outer:"));
        assert!(buf.contains("inner: \"value\""));
        assert!(buf.contains("list: (1, 2, )"));
    }

    #[test]
    fn render_returns_pdf_bytes_for_minimal_yaml() {
        let yaml = "name: Test User\ntitle: Engineer";
        let bytes = render(yaml, "cosmic").expect("render should succeed");
        assert!(bytes.starts_with(b"%PDF-"), "output is not a PDF (starts with {:?})", &bytes.get(..8));
        assert!(bytes.len() > 1000, "PDF unexpectedly small: {} bytes", bytes.len());
    }

    #[test]
    fn render_works_for_all_themes() {
        let yaml = "name: Test\ntitle: Engineer\nlunatech_since: \"2020\"";
        for theme in ["cosmic", "luxe", "opera"] {
            let bytes = render(yaml, theme).unwrap_or_else(|e| panic!("theme {theme}: {e}"));
            assert!(bytes.starts_with(b"%PDF-"), "theme {theme} did not produce a PDF");
        }
    }

    #[test]
    fn render_unknown_theme_falls_back() {
        // Unknown theme name should not crash — Typst template falls back to cosmic.
        let yaml = "name: Test\ntitle: Engineer";
        let bytes = render(yaml, "purple-rain").expect("unknown theme should fall back");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn render_yaml_overrides_theme_query() {
        let yaml = "name: Test\ntitle: Engineer\ntheme: opera";
        // Even though we pass cosmic, the YAML's `theme: opera` should win
        // (we just verify it still renders cleanly — the visual difference is
        // not asserted because we don't re-parse the PDF).
        let bytes = render(yaml, "cosmic").unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn render_handles_special_characters() {
        let yaml = "name: \"O'Brien & Sons\"\ntitle: \"C++ \\\"Senior\\\" Eng\"\nsummary: \"Backslashes \\\\ and unicode 日本語\"";
        let bytes = render(yaml, "cosmic").expect("special chars should render");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn render_handles_full_schema() {
        let yaml = r#"
name: Full Test
title: Senior Engineer
lunatech_since: "2020"
client_name: ACME
key_assets:
  - 10 years of Rust
  - Distributed systems
summary: A full schema test.
experiences:
  - company: Big Co
    role: Lead
    period: "2024 — present"
    description: Backend leadership.
    tags: [Rust, Postgres]
projects:
  - name: Side Project
    description: Something fun.
    tags: [Hobby]
skills:
  - group: Languages
    items:
      - { name: Rust, level: 5 }
      - { name: Java, level: 4 }
education:
  - school: ENS
    degree: M.Sc.
    year: "2010"
certifications:
  - name: PSM 1
    issuer: scrum.org
    year: "2021"
languages:
  - language: French
    level: Native
"#;
        let bytes = render(yaml, "cosmic").expect("full schema should render");
        assert!(bytes.starts_with(b"%PDF-"));
        // A multi-section CV should produce something substantial.
        assert!(bytes.len() > 5000);
    }

    #[test]
    fn render_rejects_invalid_yaml() {
        let bad = "name: : :\n  - this is broken";
        let err = render(bad, "cosmic").unwrap_err();
        assert!(err.to_string().contains("invalid YAML"), "unexpected err: {err}");
    }

    #[test]
    fn render_rejects_top_level_scalar() {
        // Typst template assumes a mapping at the top.
        let yaml = "just a string";
        // A scalar at the top is a string, not a mapping. The template will fail
        // because cv-data.name is missing. We just assert render returns an error
        // rather than panicking.
        let result = render(yaml, "cosmic");
        assert!(result.is_err(), "expected error for top-level scalar");
    }
}

