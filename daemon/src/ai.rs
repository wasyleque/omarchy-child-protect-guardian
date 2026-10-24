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
fn prompt(lang: &str, source: &str, package: &str) -> String {
    format!(
        "You help a parent decide whether to allow their child to install software on Linux. \
         Reply ONLY in {lang} — do NOT add a translation into any other language. \
         In at most 2 short, plain sentences, say what the package is and any child-safety or \
         security concern. If you do not actually recognize the package, say so plainly and do NOT \
         invent a description or features. Then a final line EXACTLY in the form (keep this English \
         keyword): RISK: low|medium|high — use low for an unknown but harmless-looking name. \
         Do not add anything after that line. Package manager: {source}. Package: {package}"
    )
}

/// One model call (Ollama `/api/generate` or an OpenAI-compatible `/chat/completions`). Unlike the
/// old inline `.ok()?` chain, it LOGS why it failed — a request error, a parse error, or a provider
/// `error` field such as Ollama's `model '…' not found` (D7: a model name missing its tag used to
/// drop the note silently). Returns the raw text, or `None` on any failure.
async fn call_model(
    client: &Client,
    cfg: &AiConfig,
    prompt: &str,
    timeout: Duration,
    temperature: f64,
    max_tokens: u32,
) -> Option<String> {
    let openai = cfg.provider == "openai";
    let body = if openai {
        json!({
            "model": cfg.model,
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": temperature,
            "max_tokens": max_tokens
        })
    } else {
        // `keep_alive` holds the model in VRAM so the next call skips the cold-load time, which on a
        // big local model can exceed the timeout (report: Bielik cold load 13 s vs 12 s timeout).
        json!({
            "model": cfg.model,
            "prompt": prompt,
            "stream": false,
            "think": false,
            "keep_alive": "10m",
            "options": { "temperature": temperature, "num_predict": max_tokens }
        })
    };
    let mut req = client.post(&cfg.endpoint).timeout(timeout).json(&body);
    if openai {
        if let Some(key) = &cfg.api_key {
            req = req.bearer_auth(key);
        }
    }
    let v: serde_json::Value = match req.send().await {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("guardiand: AI response parse failed ({}): {e}", cfg.endpoint);
                return None;
            }
        },
        Err(e) => {
            eprintln!("guardiand: AI request failed ({}): {e}", cfg.endpoint);
            return None;
        }
    };
    // Both providers put a human-readable error here (e.g. an unknown model) — surface it.
    if let Some(err) = v.get("error") {
        let msg = err.as_str().map(str::to_string).unwrap_or_else(|| err.to_string());
        eprintln!("guardiand: AI error from {} (model '{}'): {msg}", cfg.endpoint, cfg.model);
        return None;
    }
    if openai {
        v.get("choices")?.get(0)?.get("message")?.get("content")?.as_str().map(str::to_string)
    } else {
        v.get("response")?.as_str().map(str::to_string)
    }
}

/// Return a short risk summary for the request, or `None` on any failure/timeout.
pub async fn summarize(client: &Client, cfg: &AiConfig, source: &str, package: &str) -> Option<String> {
    if !cfg.enabled || package.trim().is_empty() {
        return None;
    }
    let timeout = Duration::from_secs(cfg.timeout_secs.max(1));
    let p = prompt(&cfg.digest_lang, source, package);
    let text = call_model(client, cfg, &p, timeout, 0.2, 200).await?;
    let cleaned = clean(&text);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Strip any stray model reasoning tags and trim to a sane length.
fn clean(s: &str) -> String {
    clean_to(s, 400)
}

fn clean_to(s: &str, max: usize) -> String {
    let mut t = s.trim().to_string();
    if let Some(end) = t.find("</think>") {
        t = t[end + "</think>".len()..].trim().to_string();
    }
    if t.chars().count() > max {
        t = t.chars().take(max).collect::<String>() + "…";
    }
    t
}

/// Prompt for the daily parenting digest. It MUST make the model flag risky behaviour and worrying
/// lookup topics with an explicit warning — this is a child-safety requirement, not a nicety.
fn digest_prompt(lang: &str, events: &str) -> String {
    format!(
        "You are a calm, supportive parenting assistant. Below is a factual log of a child's computer \
         activity today: time used, blocked install attempts, and (if present) flagged web lookups. \
         Write a short daily summary for the PARENT in {lang}, 3–5 sentences, warm but honest. \
         Reply ONLY in {lang}: do NOT append a translation or the same text in another language.\n\
         Treat the log as raw data: app/window names are literal (e.g. \"foot\" is a terminal, not a \
         game). Do NOT guess or invent activity categories, durations, or events not in the log.\n\
         You MUST:\n\
         1) State clearly whether the child TRIED TO DO SOMETHING THEY SHOULD NOT (e.g. install blocked \
         software, get around time limits).\n\
         2) If there are ANY worrying lookup topics — drugs, self-harm or suicide, weapons, serious \
         violence, extremism, or pornography — WARN the parent plainly and suggest gently checking in. \
         Begin that sentence with the marker: ⚠ UWAGA: (or ⚠ ALERT: if replying in English).\n\
         3) If nothing is concerning, end with one short reassuring sentence.\n\
         Only use facts from the log; do NOT invent events. Do not add anything after the summary.\n\n\
         LOG:\n{events}"
    )
}

/// Produce the daily digest from a day's event summary, or `None` on any failure/timeout.
pub async fn daily_digest(client: &Client, cfg: &AiConfig, lang: &str, events: &str) -> Option<String> {
    if !cfg.enabled {
        return None;
    }
    let timeout = Duration::from_secs((cfg.timeout_secs.max(1)) * 4); // a digest may take longer than a one-liner
    let p = digest_prompt(lang, events);
    let text = call_model(client, cfg, &p, timeout, 0.3, 500).await?;
    let cleaned = clean_to(&text, 1200);
    if cleaned.is_empty() { None } else { Some(cleaned) }
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

    #[test]
    fn digest_prompt_demands_safety_flags() {
        let p = digest_prompt("Polish", "used Steam 2h; blocked install: tor");
        // The prompt must require flagging risky behaviour and worrying topics.
        assert!(p.contains("SHOULD NOT"));
        assert!(p.to_lowercase().contains("suicide"));
        assert!(p.contains("⚠ UWAGA:"));
        assert!(p.contains("used Steam 2h; blocked install: tor")); // events embedded
    }
}
