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
const MAX_TOKENS: u32 = 16000;
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

/// JSON schema sent to Claude via `output_config.format`. Constrains the
/// response so we can deserialize it directly without prose-parsing.
fn output_schema() -> Value {
    // Note: the Anthropic structured-outputs API rejects numerical constraints
    // (minimum/maximum/multipleOf) and string-length constraints. The 1-10 bound
    // is enforced via the description instead — Claude respects it reliably.
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
            },
            "improved_yaml": {
                "type": "string",
                "description": "Rewritten CV in the same YAML schema as the input, with [TO COMPLETE: ...] placeholders. Empty string if not applicable."
            }
        },
        "required": ["overall_score", "verdict", "language", "report_markdown", "improved_yaml"],
        "additionalProperties": false
    })
}

pub async fn review(cfg: &AnthropicConfig, yaml: &str) -> Result<Review> {
    // System prompt is stable across requests — mark it for prompt caching.
    // Below the 4096-token Opus 4.7 minimum the cache silently no-ops, which
    // is fine; if the skill grows, caching kicks in automatically.
    let body = json!({
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "thinking": { "type": "adaptive" },
        "output_config": {
            "effort": "high",
            "format": {
                "type": "json_schema",
                "schema": output_schema(),
            }
        },
        "system": [{
            "type": "text",
            "text": cfg.system_prompt.as_str(),
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{
            "role": "user",
            "content": format!(
                "The CV below is provided in YAML format. Apply the cv-reviewer skill to it and return your structured review.\n\n```yaml\n{yaml}\n```"
            )
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

    if text_accum.is_empty() {
        return Err(anyhow!(
            "Anthropic streaming response had no text content block"
        ));
    }

    serde_json::from_str::<Review>(&text_accum).context("parsing Review JSON from Claude")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_lists_all_required_fields() {
        let schema = output_schema();
        let required = schema.get("required").and_then(Value::as_array).unwrap();
        let names: Vec<_> = required.iter().filter_map(Value::as_str).collect();
        assert!(names.contains(&"overall_score"));
        assert!(names.contains(&"verdict"));
        assert!(names.contains(&"language"));
        assert!(names.contains(&"report_markdown"));
        assert!(names.contains(&"improved_yaml"));
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
