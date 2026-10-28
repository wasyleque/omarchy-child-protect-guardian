//! Parent-selectable autonomy of the local AI models (beta).
//!
//! The ONNX models live in a separate sandboxed sidecar (`guardian-ai`, no network, low priority). This
//! module asks it for a traffic light for a package and turns that, together with the parent's mode, into
//! either an automatic decision or "ask the parent as usual". Principles:
//!  * **fail-closed toward the parent**: any doubt (sidecar down, timeout, unknown package, always-ask
//!    list, yellow in `quiet`) means ASK — the model can never approve on a guess;
//!  * the model's answer is only used when its age hint agrees with the colour (stricter wins);
//!  * nothing but package name + verdict is ever recorded; every automatic decision is audited.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::config::{AutonomyConfig, AutonomyMode, YellowPolicy};
use crate::request::{Decision, InstallSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Green,
    Yellow,
    Red,
    /// The sidecar could not identify the package (no description found) → never auto-decided.
    Unknown,
}

impl Light {
    pub fn as_str(&self) -> &'static str {
        match self {
            Light::Green => "green",
            Light::Yellow => "yellow",
            Light::Red => "red",
            Light::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rating {
    pub light: Light,
    pub min_age_hint: u8,
}

/// What to do with a held request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Answer now without bothering the parent (it will appear in the evening digest).
    Decide { decision: Decision, light: Light, min_age_hint: u8 },
    /// Hold it for the parent as usual; `why` is for the log only.
    Ask { why: &'static str },
}

/// Pure decision rule — unit-tested, no I/O.
pub fn decide(cfg: &AutonomyConfig, package: &str, rating: Option<&Rating>) -> Outcome {
    if cfg.mode == AutonomyMode::Ask {
        return Outcome::Ask { why: "mode ask" };
    }
    let lower = package.to_lowercase();
    if cfg.always_ask.iter().any(|w| !w.is_empty() && lower.contains(&w.to_lowercase())) {
        return Outcome::Ask { why: "always-ask list" };
    }
    let r = match rating {
        Some(r) => r,
        None => return Outcome::Ask { why: "model unavailable" },
    };
    // Stricter wins: a "green" whose age hint is above the child's age is not green.
    let light = match r.light {
        Light::Green if r.min_age_hint > cfg.child_age => Light::Yellow,
        l => l,
    };
    let decide = |decision| Outcome::Decide { decision, light, min_age_hint: r.min_age_hint };
    match (light, cfg.mode) {
        (Light::Unknown, _) => Outcome::Ask { why: "package not recognised" },
        (Light::Green, _) => decide(Decision::Allow),
        (Light::Yellow, AutonomyMode::Auto) if cfg.yellow == YellowPolicy::Deny => decide(Decision::Deny),
        (Light::Red, AutonomyMode::Auto) => decide(Decision::Deny),
        _ => Outcome::Ask { why: "needs the parent" },
    }
}

#[derive(Deserialize)]
struct SidecarReply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    light: String,
    #[serde(default)]
    min_age_hint: u8,
}

/// What survives a restart: the parent's chosen mode and the extra "always ask me about" fragments.
#[derive(Debug, Default, serde::Serialize, Deserialize)]
struct State {
    mode: Option<AutonomyMode>,
    #[serde(default)]
    extra_ask: Vec<String>,
}

/// Live autonomy state shared by the IPC server and the digest task.
pub struct Autonomy {
    cfg: Mutex<AutonomyConfig>,
    state_path: PathBuf,
}

impl Autonomy {
    /// `base` is the policy `[autonomy]` block; the saved state (mode set by `guardian-ctl autonomy`,
    /// extra always-ask fragments) overrides/extends it so the parent's choices survive restarts.
    pub fn new(base: AutonomyConfig, state_path: PathBuf) -> Self {
        let mut cfg = base;
        if let Ok(txt) = std::fs::read_to_string(&state_path) {
            if let Ok(st) = serde_json::from_str::<State>(&txt) {
                if let Some(m) = st.mode {
                    cfg.mode = m;
                }
                cfg.always_ask.extend(st.extra_ask);
            }
        }
        Self { cfg: Mutex::new(cfg), state_path }
    }

    pub fn config(&self) -> AutonomyConfig {
        self.cfg.lock().unwrap().clone()
    }

    pub fn mode(&self) -> AutonomyMode {
        self.cfg.lock().unwrap().mode
    }

    fn save(&self, cfg: &AutonomyConfig, base_len: usize) -> std::io::Result<()> {
        if let Some(dir) = self.state_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let st = State { mode: Some(cfg.mode), extra_ask: cfg.always_ask.iter().skip(base_len).cloned().collect() };
        std::fs::write(&self.state_path, serde_json::to_string(&st).unwrap_or_default())
    }

    pub fn set_mode(&self, mode: AutonomyMode) -> std::io::Result<()> {
        let mut c = self.cfg.lock().unwrap();
        c.mode = mode;
        let base = AutonomyConfig::default().always_ask.len();
        self.save(&c, base.min(c.always_ask.len()))
    }

    /// From now on this package (name fragment) always goes to the parent — the way to "undo" a model
    /// denial: the child asks again and the parent gets the normal card.
    pub fn ask_about(&self, fragment: &str) -> std::io::Result<()> {
        let frag = fragment.trim().to_lowercase();
        if frag.len() < 2 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "name too short"));
        }
        let mut c = self.cfg.lock().unwrap();
        if !c.always_ask.iter().any(|w| w.eq_ignore_ascii_case(&frag)) {
            c.always_ask.push(frag);
        }
        let base = AutonomyConfig::default().always_ask.len();
        self.save(&c, base.min(c.always_ask.len()))
    }

    /// Ask the sidecar for a rating. `None` on ANY failure (caller then asks the parent).
    pub async fn rate(&self, source: InstallSource, package: &str) -> Option<Rating> {
        let cfg = self.config();
        let req = serde_json::json!({
            "op": "rate_package", "age": cfg.child_age, "source": source.as_str(), "name": package,
        });
        let reply = tokio::time::timeout(
            Duration::from_secs(cfg.timeout_secs.max(1)),
            sidecar_call(&cfg.sidecar_socket, &req.to_string()),
        )
        .await
        .ok()?
        .map_err(|e| eprintln!("guardiand: guardian-ai sidecar: {e}"))
        .ok()?;
        if !reply.ok {
            return None;
        }
        let light = match reply.light.as_str() {
            "green" => Light::Green,
            "yellow" => Light::Yellow,
            "red" => Light::Red,
            _ => Light::Unknown,
        };
        Some(Rating { light, min_age_hint: reply.min_age_hint })
    }
}

async fn sidecar_call(sock: &Path, line: &str) -> std::io::Result<SidecarReply> {
    let mut s = UnixStream::connect(sock).await?;
    s.write_all(line.as_bytes()).await?;
    s.write_all(b"\n").await?;
    let mut out = String::new();
    BufReader::new(s).read_line(&mut out).await?;
    serde_json::from_str(&out).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: AutonomyMode) -> AutonomyConfig {
        AutonomyConfig { mode, child_age: 9, ..AutonomyConfig::default() }
    }
    fn r(light: Light, hint: u8) -> Rating {
        Rating { light, min_age_hint: hint }
    }
    fn allow(l: Light, h: u8) -> Outcome {
        Outcome::Decide { decision: Decision::Allow, light: l, min_age_hint: h }
    }
    fn deny(l: Light, h: u8) -> Outcome {
        Outcome::Decide { decision: Decision::Deny, light: l, min_age_hint: h }
    }

    #[test]
    fn ask_mode_never_decides() {
        assert!(matches!(decide(&cfg(AutonomyMode::Ask), "tuxmath", Some(&r(Light::Green, 7))), Outcome::Ask { .. }));
    }

    #[test]
    fn quiet_approves_only_green() {
        let c = cfg(AutonomyMode::Quiet);
        assert_eq!(decide(&c, "tuxmath", Some(&r(Light::Green, 7))), allow(Light::Green, 7));
        assert!(matches!(decide(&c, "x", Some(&r(Light::Yellow, 12))), Outcome::Ask { .. }));
        assert!(matches!(decide(&c, "x", Some(&r(Light::Red, 18))), Outcome::Ask { .. }));
    }

    #[test]
    fn auto_decides_green_red_and_yellow_per_policy() {
        let mut c = cfg(AutonomyMode::Auto);
        assert_eq!(decide(&c, "tuxmath", Some(&r(Light::Green, 7))), allow(Light::Green, 7));
        assert_eq!(decide(&c, "x", Some(&r(Light::Red, 18))), deny(Light::Red, 18));
        assert_eq!(decide(&c, "x", Some(&r(Light::Yellow, 12))), deny(Light::Yellow, 12));
        c.yellow = YellowPolicy::Ask;
        assert!(matches!(decide(&c, "x", Some(&r(Light::Yellow, 12))), Outcome::Ask { .. }));
    }

    #[test]
    fn doubt_always_asks_the_parent() {
        let c = cfg(AutonomyMode::Auto);
        assert!(matches!(decide(&c, "x", None), Outcome::Ask { .. })); // sidecar down / timeout
        assert!(matches!(decide(&c, "x", Some(&r(Light::Unknown, 0))), Outcome::Ask { .. }));
    }

    #[test]
    fn always_ask_list_beats_a_green_light() {
        let c = cfg(AutonomyMode::Auto);
        assert!(matches!(decide(&c, "Discord-Canary", Some(&r(Light::Green, 7))), Outcome::Ask { .. }));
        assert!(matches!(decide(&c, "org.torproject.torbrowser", Some(&r(Light::Green, 7))), Outcome::Ask { .. }));
    }

    #[test]
    fn green_above_childs_age_is_not_green() {
        // hint 12 > age 9 → treated as yellow: quiet asks, auto denies
        assert!(matches!(decide(&cfg(AutonomyMode::Quiet), "x", Some(&r(Light::Green, 12))), Outcome::Ask { .. }));
        assert_eq!(decide(&cfg(AutonomyMode::Auto), "x", Some(&r(Light::Green, 12))), deny(Light::Yellow, 12));
    }

    #[test]
    fn mode_persists_across_restarts() {
        let p = std::env::temp_dir().join(format!("guardian_autonomy_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let a = Autonomy::new(AutonomyConfig::default(), p.clone());
        assert_eq!(a.mode(), AutonomyMode::Ask);
        a.set_mode(AutonomyMode::Quiet).unwrap();
        a.ask_about("SomeGame").unwrap();
        let b = Autonomy::new(AutonomyConfig::default(), p.clone());
        assert_eq!(b.mode(), AutonomyMode::Quiet);
        assert!(b.config().always_ask.contains(&"somegame".to_string()));
        let _ = std::fs::remove_file(&p);
    }
}
