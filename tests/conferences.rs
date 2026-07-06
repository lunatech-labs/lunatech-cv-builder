//! Integration tests for the `conferences` section in the Typst renderer.
//!
//! These exercise the real production seam, `cv_builder::pdf::render`, the same
//! function the PDF HTTP handler calls. Each case drives the full
//! YAML -> Typst dict -> compile -> PDF path and asserts the output is a PDF.
//!
//! No Postgres or HTTP is needed, so these are plain `#[test]` functions.
//!
//! The assertion (`starts_with b"%PDF-"`) proves the Typst branch compiled and
//! emitted a PDF. It does NOT prove the section landed in the right column with
//! the right heading — that is visual, covered manually per the plan. What these
//! pin is: each placement value drives a branch that compiles cleanly, and the
//! optional-field guards inside `project-block` hold for full and name-only
//! entries.

fn assert_is_pdf(bytes: &[u8], context: &str) {
    assert!(
        bytes.starts_with(b"%PDF-"),
        "{context}: output is not a PDF (starts with {:?})",
        &bytes.get(..8)
    );
    assert!(
        bytes.len() > 1000,
        "{context}: PDF unexpectedly small: {} bytes",
        bytes.len()
    );
}

/// AC1 (side default): a `conferences` list with no `conferences_placement`
/// key renders via the SIDE branch and compiles.
#[test]
fn conferences_no_placement_renders_side() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences:
  - name: Devoxx France
    subtitle: Speaker
    description: A talk on distributed systems.
    tags: [Rust, Kubernetes]
"#;
    let bytes = render(yaml, "lunatech").expect("side default should render");
    assert_is_pdf(&bytes, "no placement (side default)");
}

/// AC2 (main placement): `conferences_placement: main` renders via the MAIN
/// branch and compiles.
#[test]
fn conferences_placement_main_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement: main
conferences:
  - name: KubeCon
    subtitle: Keynote
    description: Cloud native at scale.
    tags: [CNCF]
"#;
    let bytes = render(yaml, "lunatech").expect("main placement should render");
    assert_is_pdf(&bytes, "placement: main");
}

/// AC5 (fallback): an unrecognised placement value falls back to the SIDE
/// branch and does not error.
#[test]
fn conferences_placement_bogus_falls_back() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement: bogus
conferences:
  - name: Sunny Tech
    description: Lightning talk.
"#;
    let bytes = render(yaml, "lunatech").expect("bogus placement should fall back and render");
    assert_is_pdf(&bytes, "placement: bogus");
}

/// AC5 (fallback): an empty placement value (parses as `none`) falls back to
/// the SIDE branch and does not error.
#[test]
fn conferences_placement_empty_falls_back() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement:
conferences:
  - name: Le Camping
    description: Workshop.
"#;
    let bytes = render(yaml, "lunatech").expect("empty placement should fall back and render");
    assert_is_pdf(&bytes, "placement: empty");
}

/// AC3 (configurable title): `conferences_title` overrides the heading. We can
/// only assert the render compiles, not the visible heading string.
#[test]
fn conferences_custom_title_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_title: "Speaking & Workshops"
conferences:
  - name: Agile Tour
    subtitle: Facilitator
"#;
    let bytes = render(yaml, "lunatech").expect("custom title should render");
    assert_is_pdf(&bytes, "custom conferences_title");
}

/// AC3 (configurable title) in MAIN placement too.
#[test]
fn conferences_custom_title_main_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement: main
conferences_title: "Speaking & Workshops"
conferences:
  - name: Agile Tour
    subtitle: Facilitator
"#;
    let bytes = render(yaml, "lunatech").expect("custom title in main should render");
    assert_is_pdf(&bytes, "custom conferences_title (main)");
}

/// AC6 (entry fields): a full entry with name + subtitle + description + tags
/// renders in the SIDE branch.
#[test]
fn conferences_full_entry_side_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences:
  - name: Devoxx France
    subtitle: Speaker
    description: A deep dive into event-driven architecture and how we scaled it.
    tags: [Kafka, Scala, Event Sourcing]
"#;
    let bytes = render(yaml, "lunatech").expect("full entry should render");
    assert_is_pdf(&bytes, "full entry (side)");
}

/// AC6 (entry fields): a full entry renders in the MAIN branch too.
#[test]
fn conferences_full_entry_main_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement: main
conferences:
  - name: Devoxx France
    subtitle: Speaker
    description: A deep dive into event-driven architecture and how we scaled it.
    tags: [Kafka, Scala, Event Sourcing]
"#;
    let bytes = render(yaml, "lunatech").expect("full entry in main should render");
    assert_is_pdf(&bytes, "full entry (main)");
}

/// AC6 (entry fields): a name-only entry (no subtitle/description/tags) renders
/// without error and without empty sub-elements.
#[test]
fn conferences_name_only_entry_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences:
  - name: Codemotion
"#;
    let bytes = render(yaml, "lunatech").expect("name-only entry should render");
    assert_is_pdf(&bytes, "name-only entry");
}

/// AC6 (entry fields): a name-only entry in MAIN placement.
#[test]
fn conferences_name_only_entry_main_renders() {
    let yaml = r#"
name: Camille Dubois
title: Engineer
conferences_placement: main
conferences:
  - name: Codemotion
"#;
    let bytes = render(yaml, "lunatech").expect("name-only entry in main should render");
    assert_is_pdf(&bytes, "name-only entry (main)");
}

/// AC4 (absent section): a CV YAML with NO `conferences` key renders unchanged.
/// No conferences heading appears in either column and all other sections
/// (summary, experiences, skills, noteworthy, certifications) render without a
/// compile error.
///
/// Red-capable: both conferences guards in `assets/cv.typ` gate on
/// `opt-arr(cv-data, "conferences").len() > 0`. If that guard were removed so a
/// branch ran unconditionally, the `for item in cv-data.conferences` loop would
/// access a key that does not exist on `cv-data` and Typst would raise a compile
/// error — `render` would return `Err` and this `.expect(...)` would panic. So
/// this pins AC4 rather than passing vacuously.
#[test]
fn no_conferences_key_renders_unchanged() {
    let yaml = r#"
name: Camille Dubois
title: Senior Platform Engineer
summary: Builds and operates distributed platforms.
experiences:
  - company: Lunatech
    role: Platform Engineer
    period: 2021 - present
    description: Led the migration to Kubernetes.
    technologies: [Rust, Kubernetes, Postgres]
skills:
  - name: Backend
    items:
      - name: Rust
        level: 5
      - name: Scala
        level: 4
noteworthy:
  - name: KubeCon Speaker
    subtitle: 2023
certifications:
  - name: CKA
    subtitle: Certified Kubernetes Administrator
"#;
    let bytes = render(yaml, "lunatech").expect("CV with no conferences key should render");
    assert_is_pdf(&bytes, "no conferences key");
}

/// AC4 (absent section): an explicit empty `conferences: []` list renders
/// nothing and does not error. The `opt-arr(...).len() > 0` guard treats an
/// empty list the same as an absent key, so neither column emits a heading.
#[test]
fn empty_conferences_list_renders_unchanged() {
    let yaml = r#"
name: Camille Dubois
title: Senior Platform Engineer
summary: Builds and operates distributed platforms.
experiences:
  - company: Lunatech
    role: Platform Engineer
    period: 2021 - present
    description: Led the migration to Kubernetes.
conferences: []
"#;
    let bytes = render(yaml, "lunatech").expect("CV with empty conferences list should render");
    assert_is_pdf(&bytes, "empty conferences list");
}

use cv_builder::pdf::render;
