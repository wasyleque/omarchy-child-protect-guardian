//! Optional AI risk-summary of an install request.
//!
//! Given a package and its source, ask a model for a short, parent-facing "what is this + risk" note
//! ending in a `RISK: low|medium|high` line. Everything here is best-effort: any error, timeout, or
//! odd response returns `None`, and the caller simply shows the request without a summary. Point it at
//! a LOCAL model (Ollama on the LAN) so package names never leave your network.

use std::time::Duration;

use reqwest::Client;
use serde_json::json;

use crate::config::AiConfig;

/// Build the prompt shown to the model. Kept tight and deterministic.
fn prompt(source: &str, package: &str) -> String {
    format!(
        "You help a parent decide whether to allow their child to install software on Linux. \
         In at most 2 short, plain sentences, say what the package is and any child-safety or \
         security concern. Then a final line EXACTLY in the form: RISK: low|medium|high. \
         Do not add anything after that line. Package manager: {source}. Package: {package}"
    )
}

/// Return a short risk summary for the request, or `None` on any failure/timeout.
pub async fn summarize(client: &Client, cfg: &AiConfig, source: &str, package: &str) -> Option<String> {
    if !cfg.enabled || package.trim().is_empty() {
        return None;
    }
    let timeout = Duration::from_secs(cfg.timeout_secs.max(1));
    let p = prompt(source, package);

    let text = match cfg.provider.as_str() {
        "openai" => {
            let body = json!({
                "model": cfg.model,
                "messages": [{ "role": "user", "content": p }],
                "temperature": 0.2,
                "max_tokens": 200
            });
            let mut req = client.post(&cfg.endpoint).timeout(timeout).json(&body);
            if let Some(key) = &cfg.api_key {
                req = req.bearer_auth(key);
            }
            let v: serde_json::Value = req.send().await.ok()?.json().await.ok()?;
            v.get("choices")?.get(0)?.get("message")?.get("content")?.as_str()?.to_string()
        }
        // default: Ollama /api/generate
        _ => {
            let body = json!({
                "model": cfg.model,
                "prompt": p,
                "stream": false,
                "think": false,
                "options": { "temperature": 0.2, "num_predict": 200 }
            });
            let v: serde_json::Value = client
                .post(&cfg.endpoint)
                .timeout(timeout)
                .json(&body)
                .send()
                .await
                .ok()?
                .json()
                .await
                .ok()?;
            v.get("response")?.as_str()?.to_string()
        }
    };

    let cleaned = clean(&text);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Strip any stray model reasoning tags and trim to a sane length.
fn clean(s: &str) -> String {
    let mut t = s.trim().to_string();
    // Remove a leading <think>...</think> block if a model emitted one despite think:false.
    if let Some(end) = t.find("</think>") {
        t = t[end + "</think>".len()..].trim().to_string();
    }
    // Keep it short enough for a phone notification.
    if t.chars().count() > 400 {
        t = t.chars().take(400).collect::<String>() + "…";
    }
    t
}

/// Pull the traffic-light out of a summary, if present (for the app to color the card).
pub fn risk_level(summary: &str) -> Option<&'static str> {
    let lower = summary.to_lowercase();
    let idx = lower.rfind("risk:")?;
    let tail = &lower[idx + 5..];
    if tail.contains("high") {
        Some("high")
    } else if tail.contains("medium") {
        Some("medium")
    } else if tail.contains("low") {
        Some("low")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_risk_level() {
        assert_eq!(risk_level("Steam is a game store.\nRISK: medium"), Some("medium"));
        assert_eq!(risk_level("something RISK: HIGH"), Some("high"));
        assert_eq!(risk_level("no verdict here"), None);
    }

    #[test]
    fn clean_strips_think_block() {
        assert_eq!(clean("<think>hmm</think>\nHello. RISK: low"), "Hello. RISK: low");
    }
}
