// Seniority scoring — Rust port of the seniority_score.py heuristic.
//
// Reads the consultant's YAML CV and returns a 0-100 score split across five
// dimensions plus a coarse level (Junior / Mid-level / Senior / Staff / Tech
// Lead / Principal). The breakdown is part of the persisted output so the
// reader can audit how the verdict was reached — the scoring grid is
// deliberately simple and tunable from the constants below.
//
// All five dimensions return a `ScoreCell { points, max, note }` so they
// compose into one struct that round-trips through JSON.

use chrono::{Datelike, Local, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const LEADERSHIP_KEYWORDS: &[&str] = &[
    "principal",
    "staff",
    "tech lead",
    "team lead",
    "lead engineer",
    "scrum master",
    "architect",
    "head of",
    "director",
    "manager",
    "engineering manager",
];

const EXTERNAL_KEYWORDS: &[&str] = &[
    "conference",
    "speaker",
    "workshop",
    "open source",
    "oss",
    "talk",
    "presented",
    "keynote",
    "devoxx",
    "javazone",
    "kubecon",
    "riviera dev",
    "j-fall",
    "blog",
    "book",
    "author",
    "meetup",
];

const SCOPE_KEYWORDS: &[(&str, f64)] = &[
    ("microservices", 1.0),
    ("microservice", 1.0),
    ("feature teams", 2.0),
    ("feature team", 2.0),
    ("repositories", 0.3),
    ("repository", 0.3),
    ("stakeholders", 0.5),
    ("stakeholder", 0.5),
    ("users", 0.05),
    ("customers", 0.1),
    ("agreements", 0.05),
    ("carriers", 0.4),
    ("warehouses", 0.4),
    ("services", 0.5),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreCell {
    pub points: u32,
    pub max: u32,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Breakdown {
    pub years_experience: ScoreCell,
    pub leadership: ScoreCell,
    pub scope: ScoreCell,
    pub external_signals: ScoreCell,
    pub title_bonus: ScoreCell,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub score: u32,
    pub level: String,
    pub years: f64,
    pub breakdown: Breakdown,
}

/// Top-level entry — feeds a YAML CV into the scoring pipeline. Invalid YAML
/// (or one that can't be parsed as a mapping) returns a zeroed report rather
/// than failing — we'd rather show a "no signal" 0/100 than block the save.
pub fn score_yaml(yaml: &str) -> Report {
    let cv: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(v) => v,
        Err(_) => return empty_report(),
    };
    let empty: Vec<serde_yaml::Value> = Vec::new();
    let experiences = cv
        .get("experiences")
        .and_then(|v| v.as_sequence())
        .unwrap_or(&empty);
    let projects = cv
        .get("projects")
        .and_then(|v| v.as_sequence())
        .unwrap_or(&empty);
    let title = cv.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let years = total_years(experiences);
    let years_cell = score_years(years);
    let leadership_cell = score_leadership(experiences);
    let scope_cell = score_scope(experiences);
    let external_cell = score_external(projects, experiences);
    let title_cell = score_title(title);

    let total = years_cell.points
        + leadership_cell.points
        + scope_cell.points
        + external_cell.points
        + title_cell.points;

    Report {
        score: total,
        level: classify(total).to_string(),
        years,
        breakdown: Breakdown {
            years_experience: years_cell,
            leadership: leadership_cell,
            scope: scope_cell,
            external_signals: external_cell,
            title_bonus: title_cell,
        },
    }
}

fn empty_report() -> Report {
    Report {
        score: 0,
        level: "Junior".into(),
        years: 0.0,
        breakdown: Breakdown {
            years_experience: ScoreCell {
                points: 0,
                max: 30,
                note: "No experiences parsed".into(),
            },
            leadership: ScoreCell {
                points: 0,
                max: 25,
                note: "No experiences parsed".into(),
            },
            scope: ScoreCell {
                points: 0,
                max: 20,
                note: "No experiences parsed".into(),
            },
            external_signals: ScoreCell {
                points: 0,
                max: 15,
                note: "No external signals".into(),
            },
            title_bonus: ScoreCell {
                points: 0,
                max: 10,
                note: "No senior+ keyword in headline title".into(),
            },
        },
    }
}

// ─────────── Date parsing ───────────

fn months_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^([a-z]{3,9})\s+(\d{4})").unwrap())
}

fn year_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(\d{4})").unwrap())
}

fn split_and_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\s+and\s+|\s*;\s*").unwrap())
}

fn split_dash_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\s+-\s+|\s+to\s+|\s+—\s+|\s+–\s+").unwrap())
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim().trim_end_matches('.');
    static MONTHS: &[(&str, u32)] = &[
        ("jan", 1),
        ("feb", 2),
        ("mar", 3),
        ("apr", 4),
        ("may", 5),
        ("jun", 6),
        ("jul", 7),
        ("aug", 8),
        ("sep", 9),
        ("oct", 10),
        ("nov", 11),
        ("dec", 12),
    ];
    if let Some(c) = months_re().captures(s) {
        let mon_full = c.get(1)?.as_str().to_lowercase();
        let mon3: &str = &mon_full[..3.min(mon_full.len())];
        for (k, m) in MONTHS {
            if *k == mon3 {
                let y: i32 = c.get(2)?.as_str().parse().ok()?;
                return NaiveDate::from_ymd_opt(y, *m, 1);
            }
        }
    }
    if let Some(c) = year_re().captures(s) {
        let y: i32 = c.get(1)?.as_str().parse().ok()?;
        return NaiveDate::from_ymd_opt(y, 1, 1);
    }
    None
}

fn parse_period(s: &str) -> Vec<(NaiveDate, NaiveDate)> {
    let mut spans = Vec::new();
    for chunk in split_and_re().split(s) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let parts: Vec<&str> = split_dash_re().split(chunk).collect();
        if parts.len() < 2 {
            continue;
        }
        let start = match parse_date(parts[0]) {
            Some(d) => d,
            None => continue,
        };
        let last = parts.last().unwrap().trim().to_lowercase();
        let end = if last.contains("present") || last.contains("now") || last.contains("current")
        {
            Local::now().naive_local().date()
        } else {
            match parse_date(parts.last().unwrap()) {
                Some(d) => d,
                None => continue,
            }
        };
        spans.push((start, end));
    }
    spans
}

fn total_years(experiences: &[serde_yaml::Value]) -> f64 {
    let mut spans: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    for exp in experiences {
        if let Some(period) = exp.get("period").and_then(|v| v.as_str()) {
            spans.extend(parse_period(period));
        }
    }
    if spans.is_empty() {
        return 0.0;
    }
    let earliest = spans.iter().map(|(s, _)| *s).min().unwrap();
    let latest = spans.iter().map(|(_, e)| *e).max().unwrap();
    let months = ((latest.year() - earliest.year()) * 12
        + (latest.month() as i32 - earliest.month() as i32))
        .max(0);
    months as f64 / 12.0
}

// ─────────── Scoring dimensions ───────────

fn score_years(years: f64) -> ScoreCell {
    let (pts, label) = if years < 2.0 {
        (5, "Junior territory")
    } else if years < 5.0 {
        (15, "Mid territory")
    } else if years < 8.0 {
        (22, "Senior territory")
    } else if years < 12.0 {
        (28, "Staff / Principal territory")
    } else {
        (30, "deep experience")
    };
    ScoreCell {
        points: pts,
        max: 30,
        note: format!("{:.1} years ({})", years, label),
    }
}

fn score_leadership(experiences: &[serde_yaml::Value]) -> ScoreCell {
    if experiences.is_empty() {
        return ScoreCell {
            points: 0,
            max: 25,
            note: "No experiences found".into(),
        };
    }
    let mut lead_count = 0usize;
    let mut scrum_master = false;
    for exp in experiences {
        let role = exp
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if LEADERSHIP_KEYWORDS.iter().any(|k| role.contains(k)) {
            lead_count += 1;
            if role.contains("scrum master") {
                scrum_master = true;
            }
        }
    }
    let total = experiences.len();
    let ratio = lead_count as f64 / total.max(1) as f64;
    let mut pts: u32 = if ratio == 0.0 {
        0
    } else if ratio < 0.34 {
        8
    } else if ratio < 0.67 {
        14
    } else if ratio < 1.0 {
        20
    } else {
        23
    };
    if scrum_master {
        pts = (pts + 2).min(25);
    }
    let mut note = format!("{}/{} roles with leadership signal", lead_count, total);
    if scrum_master {
        note.push_str(" + Scrum Master");
    }
    ScoreCell {
        points: pts,
        max: 25,
        note,
    }
}

fn score_scope(experiences: &[serde_yaml::Value]) -> ScoreCell {
    if experiences.is_empty() {
        return ScoreCell {
            points: 0,
            max: 20,
            note: "No experiences found".into(),
        };
    }
    // Pre-build one regex per scope keyword so we don't recompile them
    // repeatedly across CVs.
    static KW_RES: OnceLock<Vec<(Regex, f64)>> = OnceLock::new();
    let kw_res = KW_RES.get_or_init(|| {
        SCOPE_KEYWORDS
            .iter()
            .map(|(kw, w)| {
                (
                    Regex::new(&format!(r"(\d[\d,]*)\+?\s+{}", regex::escape(kw))).unwrap(),
                    *w,
                )
            })
            .collect()
    });
    static TEAM_RE: OnceLock<Regex> = OnceLock::new();
    let team_re = TEAM_RE
        .get_or_init(|| Regex::new(r"(\d+)\s+engineer\s+(team|platform|squad)").unwrap());

    let mut total_score = 0.0f64;
    let mut team_sizes: Vec<u32> = Vec::new();
    for exp in experiences {
        let desc = exp
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        for (re, weight) in kw_res {
            for caps in re.captures_iter(&desc) {
                let raw = caps.get(1).unwrap().as_str().replace(',', "");
                let n: f64 = raw.parse().unwrap_or(0.0);
                total_score += (n * weight).min(6.0);
            }
        }
        if let Some(c) = team_re.captures(&desc) {
            let size: u32 = c.get(1).unwrap().as_str().parse().unwrap_or(0);
            team_sizes.push(size);
            total_score += (size as f64 * 0.5).min(4.0);
        }
    }
    let pts = (total_score as u32).min(20);
    let note = if team_sizes.is_empty() {
        "scope signals from descriptions".to_string()
    } else {
        let mut sorted: Vec<u32> = team_sizes.into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        sorted.sort_unstable();
        format!("team sizes encountered: {:?}", sorted)
    };
    ScoreCell {
        points: pts,
        max: 20,
        note,
    }
}

fn score_external(
    projects: &[serde_yaml::Value],
    experiences: &[serde_yaml::Value],
) -> ScoreCell {
    let mut blob = String::new();
    for p in projects {
        if let Some(s) = p.get("name").and_then(|v| v.as_str()) {
            blob.push_str(&s.to_lowercase());
            blob.push(' ');
        }
        if let Some(s) = p.get("description").and_then(|v| v.as_str()) {
            blob.push_str(&s.to_lowercase());
            blob.push(' ');
        }
    }
    for e in experiences {
        if let Some(s) = e.get("description").and_then(|v| v.as_str()) {
            blob.push_str(&s.to_lowercase());
            blob.push(' ');
        }
    }
    let hits = EXTERNAL_KEYWORDS.iter().filter(|kw| blob.contains(*kw)).count();
    let (pts, note) = if hits == 0 {
        (
            0,
            "No external signals (no conferences, OSS, etc.)".to_string(),
        )
    } else if hits < 3 {
        (5, format!("{} external signals (light)", hits))
    } else if hits < 6 {
        (10, format!("{} external signals", hits))
    } else {
        (
            15,
            format!("{} external signals (strong public presence)", hits),
        )
    };
    ScoreCell {
        points: pts,
        max: 15,
        note,
    }
}

fn score_title(title: &str) -> ScoreCell {
    let t = title.to_lowercase();
    let (pts, note) = if ["principal", "distinguished", "fellow"]
        .iter()
        .any(|k| t.contains(k))
    {
        (10, "Principal-level title")
    } else if ["staff", " lead"].iter().any(|k| t.contains(k)) || t.ends_with("lead") {
        (7, "Staff / Lead title")
    } else if t.contains("senior") {
        (4, "Senior title")
    } else {
        (0, "No senior+ keyword in headline title")
    };
    ScoreCell {
        points: pts,
        max: 10,
        note: note.to_string(),
    }
}

fn classify(total: u32) -> &'static str {
    match total {
        0..=24 => "Junior",
        25..=44 => "Mid-level",
        45..=64 => "Senior",
        65..=79 => "Staff / Tech Lead",
        _ => "Principal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Report {
        score_yaml(s)
    }

    #[test]
    fn empty_yaml_lands_in_junior() {
        // The years dimension gives 5 baseline points to anything < 2 years
        // (matching the Python heuristic), so an empty CV scores 5 / Junior.
        let r = yaml("");
        assert!(r.score < 25);
        assert_eq!(r.level, "Junior");
    }

    #[test]
    fn invalid_yaml_lands_in_junior() {
        let r = yaml("not: a [valid: ::cv");
        assert!(r.score < 25);
        assert_eq!(r.level, "Junior");
    }

    #[test]
    fn parse_date_handles_month_year() {
        assert_eq!(
            parse_date("Sep 2023"),
            Some(NaiveDate::from_ymd_opt(2023, 9, 1).unwrap())
        );
        assert_eq!(
            parse_date("2020"),
            Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap())
        );
    }

    #[test]
    fn parse_period_present() {
        let spans = parse_period("Sep 2023 - present");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].1 >= NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    }

    #[test]
    fn classify_buckets() {
        assert_eq!(classify(0), "Junior");
        assert_eq!(classify(24), "Junior");
        assert_eq!(classify(25), "Mid-level");
        assert_eq!(classify(44), "Mid-level");
        assert_eq!(classify(45), "Senior");
        assert_eq!(classify(64), "Senior");
        assert_eq!(classify(65), "Staff / Tech Lead");
        assert_eq!(classify(79), "Staff / Tech Lead");
        assert_eq!(classify(80), "Principal");
        assert_eq!(classify(100), "Principal");
    }

    #[test]
    fn senior_full_stack_scores_high() {
        // Roughly modelled on the cv-empty.yaml schema. Should land at
        // least in Senior territory.
        let cv = r#"
title: Senior Software Engineer
experiences:
  - role: Tech Lead
    period: "Sep 2018 - present"
    description: "Led 8 engineer team across 12 microservices and 4 stakeholders."
  - role: Senior Engineer
    period: "Jan 2014 - Aug 2018"
    description: "Built feature teams of 5 engineers."
projects:
  - name: Open source library
    description: "Spoke at Devoxx 2022 on this open source project."
"#;
        let r = yaml(cv);
        assert!(
            r.score >= 45,
            "expected Senior+ score, got {} ({})",
            r.score,
            r.level
        );
    }
}
