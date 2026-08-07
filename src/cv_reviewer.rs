// Calls the Anthropic Messages API with the cv-reviewer skill as the system
// prompt and a user-supplied CV YAML, then parses Claude's structured JSON
// response into a `Review`. Raw HTTP via reqwest — Rust has no official SDK.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MODEL: &str = "claude-opus-4-7";
// Applies to each of the two calls separately, and covers *both* adaptive
// thinking and the response body. Measured against a 12-entry CV (9
// experiences + 3 projects, ~19k chars of input YAML): `report_markdown` runs
// about 5k tokens and the rewritten `improved_yaml` 4.5-5.5k. When both shared
// one request that was ~10k of body before a single thinking token was spent,
// and at 16000 the remainder was not enough for `effort: high` — the response
// was cut mid-string and the truncated JSON failed to parse.
//
// A ceiling, not a spend: only generated tokens are billed. Kept at 32000
// rather than higher because output-token rate limits reserve against
// `max_tokens` at request time, and `batch_review` dispatches reviews
// concurrently.
const MAX_TOKENS: u32 = 32000;
// Streaming keeps the connection alive while Claude works through a long CV
// (8+ experiences with effort: high routinely take 5-15 minutes). Without
// streaming we'd hit the reqwest timeout *and* Anthropic's own ~10-minute
// server-side limit on non-streaming responses. 25 minutes is a generous
// upper bound; the request normally completes in well under that.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25 * 60);
const SKILL_PATH: &str = "assets/skills/cv-reviewer/SKILL.md";

/// Configuration shared across requests. Created once at startup.
#[derive(Clone)]
pub struct AnthropicConfig {
    pub api_key: Arc<String>,
    pub system_prompt: Arc<String>,
    pub http: reqwest::Client,
}

impl AnthropicConfig {
    pub fn from_env_and_skill() -> Result<Option<Self>> {
        let api_key = match std::env::var("ANTHROPIC_API_KEY") {
            Ok(key) if !key.trim().is_empty() => key,
            _ => return Ok(None),
        };
        let system_prompt = std::fs::read_to_string(SKILL_PATH)
            .with_context(|| format!("reading cv-reviewer skill at {SKILL_PATH}"))?;
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("building reqwest client")?;
        Ok(Some(Self {
            api_key: Arc::new(api_key),
            system_prompt: Arc::new(system_prompt),
            http,
        }))
    }
}

/// What Claude returns and what we persist as `latest_review`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Review {
    pub overall_score: u8,
    pub verdict: String,
    pub language: String,
    pub report_markdown: String,
    pub improved_yaml: String,
}

/// Call 1's payload: the analysis, without the rewritten CV.
#[derive(Debug, Deserialize)]
struct ReportPart {
    overall_score: u8,
    verdict: String,
    language: String,
    report_markdown: String,
}

/// Call 2's payload: the rewritten CV on its own.
#[derive(Debug, Deserialize)]
struct RewritePart {
    improved_yaml: String,
}

// Note: the Anthropic structured-outputs API rejects numerical constraints
// (minimum/maximum/multipleOf) and string-length constraints. The 0-100 bound
// is enforced via the description instead — Claude respects it reliably.

/// Schema for call 1 (Steps 1-3 of the skill): score, verdict, language and
/// the per-project analysis.
fn report_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "overall_score": {
                "type": "integer",
                "description": "Score from 0 (unusable) to 100 (perfect). Use the full range so two similarly-good CVs can still be told apart by 5-10 points. CRITICAL: per-project length deductions are mechanical — for every project with description length > 260 words, subtract 8 points (or 12 if > 340 words) from the score, then sum across projects. A CV with several overlong projects MUST land below 70 even if every other criterion is satisfied."
            },
            "verdict": {
                "type": "string",
                "enum": ["client_ready", "minor_improvements", "major_rework"],
                "description": "One-shot verdict bucket."
            },
            "language": {
                "type": "string",
                "description": "Language of the CV and the review (ISO 639-1: 'fr', 'en', etc.)."
            },
            "report_markdown": {
                "type": "string",
                "description": "Full per-project analysis report in markdown, in the CV's language."
            }
        },
        "required": ["overall_score", "verdict", "language", "report_markdown"],
        "additionalProperties": false
    })
}

/// Schema for call 2 (Step 4 of the skill): the rewritten CV alone.
fn rewrite_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "improved_yaml": {
                "type": "string",
                "description": "Rewritten CV in the same YAML schema as the input, with [TO COMPLETE: ...] placeholders. Empty string if not applicable."
            }
        },
        "required": ["improved_yaml"],
        "additionalProperties": false
    })
}

/// Reviews a CV in two requests: the analysis, then the rewrite.
///
/// They were one request until a long CV exhausted the token budget mid-JSON.
/// `report_markdown` and `improved_yaml` are each several thousand tokens, and
/// both had to fit alongside `effort: high` thinking under a single
/// `max_tokens`. Split, each call gets the whole budget.
///
/// The rewrite is sequential rather than concurrent on purpose: it is given the
/// report, so it rewrites against the weaknesses the analysis just identified
/// instead of forming its own opinion of them.
///
/// A failed rewrite degrades rather than fails. The report is the load-bearing
/// half of a review and is worth returning on its own; callers see an empty
/// `improved_yaml`, which the schema already documents as valid.
pub async fn review(cfg: &AnthropicConfig, yaml: &str) -> Result<Review> {
    let report_json = stream_json(
        cfg,
        report_schema(),
        format!(
            "The CV below is provided in YAML format. Apply Steps 1-3 of the \
             cv-reviewer skill to it (analysis, scoring and the per-project \
             report) and return your structured review. Do not rewrite the CV \
             yet — that is a separate request.\n\n```yaml\n{yaml}\n```"
        ),
        "review report",
    )
    .await?;

    let report: ReportPart =
        serde_json::from_str(&report_json).context("parsing Review JSON from Claude")?;

    let improved_yaml = match rewrite(cfg, yaml, &report.report_markdown).await {
        Ok(y) => y,
        Err(e) => {
            // Deliberately not fatal — see the doc comment above.
            tracing::warn!("CV rewrite failed, returning report only: {:#}", e);
            String::new()
        }
    };

    Ok(Review {
        overall_score: report.overall_score,
        verdict: report.verdict,
        language: report.language,
        report_markdown: report.report_markdown,
        improved_yaml,
    })
}

/// Call 2: Step 4 of the skill, handed the CV and the report that call 1 just
/// produced.
async fn rewrite(cfg: &AnthropicConfig, yaml: &str, report_markdown: &str) -> Result<String> {
    let rewrite_json = stream_json(
        cfg,
        rewrite_schema(),
        format!(
            "Below are a CV in YAML format and the review report already \
             produced for it. Apply Step 4 of the cv-reviewer skill: return the \
             improved CV in the same YAML schema as the input, trimmed to the \
             150-200 words per entry the skill requires, addressing the \
             weaknesses the report identifies, and marking genuine gaps with \
             [TO COMPLETE: ...] placeholders. Keep the CV's original \
             language.\n\n```yaml\n{yaml}\n```\n\n## Review report\n\n{report_markdown}"
        ),
        "CV rewrite",
    )
    .await?;

    let part: RewritePart =
        serde_json::from_str(&rewrite_json).context("parsing rewritten CV JSON from Claude")?;
    Ok(part.improved_yaml)
}

/// Runs one schema-constrained streaming request and returns the raw JSON text
/// of the response. Owns the SSE protocol, the truncation check and error
/// mapping, so the two call sites above stay declarative.
///
/// `label` names the call in error messages, since a failure now has two
/// possible origins.
async fn stream_json(
    cfg: &AnthropicConfig,
    schema: Value,
    user_content: String,
    label: &str,
) -> Result<String> {
    // System prompt is stable across requests — mark it for prompt caching.
    // Below the 4096-token Opus 4.7 minimum the cache silently no-ops, which
    // is fine; if the skill grows, caching kicks in automatically. With two
    // calls per review the cache now earns its keep either way: the second
    // request reuses the first's cached system prompt.
    let body = json!({
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "thinking": { "type": "adaptive" },
        "output_config": {
            "effort": "high",
            "format": {
                "type": "json_schema",
                "schema": schema,
            }
        },
        "system": [{
            "type": "text",
            "text": cfg.system_prompt.as_str(),
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{
            "role": "user",
            "content": user_content
        }]
    });

    let mut resp = cfg
        .http
        .post(ANTHROPIC_URL)
        .header("x-api-key", cfg.api_key.as_str())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .json(&body)
        .send()
        .await
        .context("calling Anthropic API")?;

    let status = resp.status();
    if !status.is_success() {
        let payload: Value = resp
            .json()
            .await
            .context("decoding Anthropic error response")?;
        let msg = payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(anyhow!("Anthropic API {}: {}", status, msg));
    }

    // Consume the SSE stream. Each event is one or more lines terminated by
    // `\n`, with the JSON payload on a `data: ...` line. We track which
    // content block is currently open and accumulate `text_delta`s on the
    // text block (where the JSON-schema-constrained structured output lands)
    // — `thinking_delta`s on the thinking block are intentionally dropped.
    let mut buf: Vec<u8> = Vec::new();
    let mut text_accum = String::new();
    let mut current_block_type: Option<String> = None;
    // Carried on `message_delta`. Without it a response cut short by the token
    // budget is indistinguishable from a complete one, and surfaces as an
    // opaque JSON parse error instead of the thing that actually went wrong.
    let mut stop_reason: Option<String> = None;

    while let Some(chunk) = resp
        .chunk()
        .await
        .context("reading Anthropic SSE stream")?
    {
        buf.extend_from_slice(&chunk);

        while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=nl).collect();
            let line = std::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
                .unwrap_or("")
                .trim_end_matches('\r');

            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<Value>(data) else {
                continue;
            };

            match event.get("type").and_then(Value::as_str) {
                Some("content_block_start") => {
                    current_block_type = event
                        .pointer("/content_block/type")
                        .and_then(Value::as_str)
                        .map(String::from);
                }
                Some("content_block_delta") => {
                    let delta_type = event.pointer("/delta/type").and_then(Value::as_str);
                    if delta_type == Some("text_delta")
                        && current_block_type.as_deref() == Some("text")
                    {
                        if let Some(t) = event.pointer("/delta/text").and_then(Value::as_str) {
                            text_accum.push_str(t);
                        }
                    }
                }
                Some("content_block_stop") => {
                    current_block_type = None;
                }
                Some("message_delta") => {
                    if let Some(r) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                        stop_reason = Some(r.to_string());
                    }
                }
                Some("error") => {
                    let msg = event
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown stream error");
                    return Err(anyhow!("Anthropic stream error: {msg}"));
                }
                _ => {}
            }
        }
    }

    // Check the stop reason before returning: a truncated response is still
    // syntactically "some JSON", so the caller's serde error would report an
    // EOF deep in a string rather than the budget exhaustion that caused it.
    if stop_reason.as_deref() == Some("max_tokens") {
        return Err(anyhow!(
            "Anthropic response for the {label} was truncated at max_tokens ({MAX_TOKENS})"
        ));
    }

    if text_accum.is_empty() {
        return Err(anyhow!(
            "Anthropic streaming response for the {} had no text content block (stop_reason: {})",
            label,
            stop_reason.as_deref().unwrap_or("none")
        ));
    }

    Ok(text_accum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required_of(schema: &Value) -> Vec<String> {
        schema
            .get("required")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect()
    }

    #[test]
    fn report_schema_lists_all_required_fields() {
        let names = required_of(&report_schema());
        assert!(names.contains(&"overall_score".to_string()));
        assert!(names.contains(&"verdict".to_string()));
        assert!(names.contains(&"language".to_string()));
        assert!(names.contains(&"report_markdown".to_string()));
    }

    #[test]
    fn rewrite_schema_lists_only_the_improved_yaml() {
        assert_eq!(required_of(&rewrite_schema()), vec!["improved_yaml"]);
    }

    /// The split exists to keep each response inside one token budget. If
    /// `improved_yaml` ever creeps back into the report schema the two calls
    /// would both carry it and the original failure returns.
    #[test]
    fn report_schema_excludes_the_rewritten_cv() {
        let names = required_of(&report_schema());
        assert!(!names.contains(&"improved_yaml".to_string()));
        assert!(
            report_schema()
                .pointer("/properties/improved_yaml")
                .is_none()
        );
    }

    /// The two schemas together must still cover every field of `Review`,
    /// otherwise assembly in `review()` silently loses one.
    #[test]
    fn the_two_schemas_together_cover_every_review_field() {
        let mut names = required_of(&report_schema());
        names.extend(required_of(&rewrite_schema()));
        for field in [
            "overall_score",
            "verdict",
            "language",
            "report_markdown",
            "improved_yaml",
        ] {
            assert!(names.contains(&field.to_string()), "missing {field}");
        }
    }

    #[test]
    fn review_roundtrips_through_json() {
        let r = Review {
            overall_score: 7,
            verdict: "minor_improvements".into(),
            language: "fr".into(),
            report_markdown: "## Profil\n...".into(),
            improved_yaml: "name: Alice\n".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: Review = serde_json::from_str(&s).unwrap();
        assert_eq!(back.overall_score, 7);
        assert_eq!(back.verdict, "minor_improvements");
    }

    #[test]
    fn from_env_returns_none_when_key_missing() {
        // Use a unique scope so we don't fight with parallel tests.
        // SAFETY: tests are single-process; this only mutates env briefly.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        let cfg = AnthropicConfig::from_env_and_skill().unwrap();
        assert!(cfg.is_none());
    }
}
