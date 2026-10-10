//! Local IPC over two Unix-domain sockets speaking newline-delimited JSON.
//!
//! * **submit socket** (`submit.sock`, mode 0666) — anything that intercepts an install (the pacman
//!   hook as root, or a user-space `flatpak` wrapper running as the child) sends one
//!   [`ClientMessage::Submit`] and blocks until the daemon replies with a [`ServerMessage::Decision`].
//!   It accepts *only* `Submit`, and enforces a per-uid cap so a local user can't flood it.
//! * **control socket** (`guardian.sock`, mode 0660) — the parent's `guardian-ctl` sends
//!   [`ClientMessage::List`] / [`ClientMessage::Resolve`]. This surface is **peer-credential checked**
//!   (SO_PEERCRED): only the daemon-owner uid (root in production) may make decisions, so a child
//!   cannot connect and approve their own request regardless of socket permissions.
//!
//! Splitting the surfaces is what lets a child-run interceptor *submit* a request while the power to
//! *decide* stays with root/the parent.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use uuid::Uuid;

/// Hard caps so a local process can't exhaust the daemon (review finding #7).
const MAX_CONNECTIONS: usize = 64; // concurrent in-flight connections across both sockets
const MAX_MSG_BYTES: u64 = 64 * 1024; // bytes a single connection may feed before EOF
const READ_TIMEOUT: Duration = Duration::from_secs(30); // max wait for the next request line

use std::sync::Mutex;

use crate::audit::{Audit, AuditEvent};
use crate::queue::Queue;
use crate::request::{Decision, DecisionVia, InstallRequest, InstallSource};
use crate::schedule::{Status as SchedStatus, TimeEngine};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientMessage {
    Submit {
        source: InstallSource,
        package: String,
        #[serde(default)]
        reason: Option<String>,
    },
    List,
    Resolve { id: Uuid, decision: Decision },
    /// Control-only: forward a tamper/integrity alert to the parent (used by the watchdog).
    Alert { message: String },
    /// Control-only: query the current screen-time schedule status.
    ScheduleStatus,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    Decision { id: Uuid, decision: Decision },
    Pending { requests: Vec<InstallRequest> },
    Resolved { id: Uuid, ok: bool },
    /// Ack for `Alert`; `ok` is false when no remote (ntfy) is configured to deliver it.
    Alerted { ok: bool },
    /// Answer for `ScheduleStatus`.
    Schedule { status: String, remaining_secs: Option<u64> },
    Error { message: String },
}

pub struct Server {
    pub queue: Arc<Queue>,
    pub decision_timeout: Duration,
    pub default_on_timeout: Decision,
    /// Optional remote push-approval; when present, each held request is also pushed to the phone.
    pub ntfy: Option<Arc<crate::ntfy::Ntfy>>,
    /// UIDs permitted to use the **control** socket (decide/list): the daemon owner (root) + parent.
    pub control_uids: Vec<u32>,
    /// Max concurrently-held requests per submitting uid (anti-flood).
    pub max_pending_per_uid: usize,
    /// Optional append-only audit log.
    pub audit: Option<Arc<Audit>>,
    /// Optional screen-time engine (for status queries).
    pub schedule: Option<Arc<Mutex<TimeEngine>>>,
}

fn bind(path: &Path, mode: u32) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    let listener =
        UnixListener::bind(path).with_context(|| format!("binding unix socket at {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    Ok(listener)
}

impl Server {
    /// Bind both sockets and serve until a fatal accept error on either.
    pub async fn run(self: Arc<Self>, control_path: &Path, submit_path: &Path) -> Result<()> {
        let control = bind(control_path, 0o660)?;
        let submit = bind(submit_path, 0o666)?;
        eprintln!(
            "guardiand: control socket {} (owner-only), submit socket {} (any uid, capped)",
            control_path.display(),
            submit_path.display()
        );
        let conns = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        tokio::select! {
            r = Arc::clone(&self).accept_loop(control, true, Arc::clone(&conns)) => r,
            r = Arc::clone(&self).accept_loop(submit, false, Arc::clone(&conns)) => r,
        }
    }

    async fn accept_loop(self: Arc<Self>, listener: UnixListener, is_control: bool, conns: Arc<Semaphore>) -> Result<()> {
        loop {
            let (stream, _addr) = listener.accept().await.context("accepting a connection")?;
            let uid = stream.peer_cred().ok().map(|c| c.uid());
            if is_control {
                match uid {
                    Some(u) if self.control_uids.contains(&u) => {}
                    Some(u) => {
                        eprintln!("guardiand: refused control connection from uid {u} (not authorized)");
                        if let Some(ntfy) = &self.ntfy {
                            ntfy.alert(
                                "ipc-blocked",
                                "Guardian: blocked control attempt",
                                &format!("A process (uid {u}) tried to control Guardian on this computer and was blocked."),
                            );
                        }
                        if let Some(audit) = &self.audit {
                            audit.record(AuditEvent::ControlBlocked { uid: u });
                        }
                        continue;
                    }
                    None => {
                        eprintln!("guardiand: refused control connection (no peer credentials)");
                        continue;
                    }
                }
            }
            let uid = uid.unwrap_or(u32::MAX);
            // Cap concurrent connections; if we're at the limit, drop this one rather than pile up.
            let permit = match Arc::clone(&conns).try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    eprintln!("guardiand: connection limit reached — dropping a connection");
                    continue;
                }
            };
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = server.handle(stream, is_control, uid).await {
                    eprintln!("guardiand: connection error: {e:#}");
                }
                drop(permit);
            });
        }
    }

    async fn handle(&self, stream: UnixStream, is_control: bool, uid: u32) -> Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        // Bound total bytes a connection may feed, so a never-terminated line can't exhaust memory.
        let mut lines = BufReader::new(read_half.take(MAX_MSG_BYTES)).lines();

        loop {
            // Bound how long we wait for each request line.
            let next = match tokio::time::timeout(READ_TIMEOUT, lines.next_line()).await {
                Ok(r) => r?,
                Err(_) => break, // read timed out
            };
            let line = match next {
                Some(l) => l,
                None => break,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let msg: ClientMessage = match serde_json::from_str(line) {
                Ok(m) => m,
                Err(e) => {
                    send(&mut write_half, &ServerMessage::Error { message: format!("bad request: {e}") }).await?;
                    continue;
                }
            };

            match (is_control, msg) {
                // ---- control socket: list / resolve only ----
                (true, ClientMessage::List) => {
                    let requests = self.queue.list();
                    send(&mut write_half, &ServerMessage::Pending { requests }).await?;
                }
                (true, ClientMessage::Resolve { id, decision }) => {
                    let ok = self.queue.resolve(id, decision, DecisionVia::LocalCtl);
                    send(&mut write_half, &ServerMessage::Resolved { id, ok }).await?;
                }
                (true, ClientMessage::Alert { message }) => {
                    eprintln!("guardiand: watchdog alert: {message}");
                    if let Some(a) = &self.audit {
                        a.record(AuditEvent::WatchdogAlert { message: message.clone() });
                    }
                    let delivered = if let Some(ntfy) = &self.ntfy {
                        ntfy.alert("watchdog", "Guardian: integrity alert", &message);
                        true
                    } else {
                        false
                    };
                    send(&mut write_half, &ServerMessage::Alerted { ok: delivered }).await?;
                }
                (true, ClientMessage::ScheduleStatus) => {
                    let (status, remaining_secs) = match &self.schedule {
                        Some(eng) => {
                            let (day, minute) = crate::schedule::local_day_minute();
                            match eng.lock().unwrap().status(day, minute) {
                                SchedStatus::Allowed { remaining_secs } => ("allowed".to_string(), remaining_secs),
                                SchedStatus::OutsideWindow => ("outside_window".to_string(), Some(0)),
                                SchedStatus::BudgetExhausted => ("budget_exhausted".to_string(), Some(0)),
                            }
                        }
                        None => ("no_schedule".to_string(), None),
                    };
                    send(&mut write_half, &ServerMessage::Schedule { status, remaining_secs }).await?;
                }
                (true, ClientMessage::Submit { .. }) => {
                    send(&mut write_half, &ServerMessage::Error {
                        message: "submit not accepted on the control socket".into(),
                    }).await?;
                }

                // ---- submit socket: submit only ----
                (false, ClientMessage::Submit { source, package, reason }) => {
                    let pkg_label = package.clone();
                    let held = self.queue.try_submit(source, package, reason, uid, self.max_pending_per_uid);
                    let (req, rx) = match held {
                        Some(v) => v,
                        None => {
                            eprintln!("guardiand: refused submit from uid {uid} (per-uid cap reached) → deny");
                            if let Some(a) = &self.audit {
                                a.record(AuditEvent::SubmitRefused {
                                    uid,
                                    source,
                                    package: pkg_label,
                                    why: "per-uid cap reached".into(),
                                });
                            }
                            send(&mut write_half, &ServerMessage::Error {
                                message: "too many pending requests".into(),
                            }).await?;
                            break;
                        }
                    };
                    eprintln!(
                        "guardiand: HELD {:?} install '{}' (id {}, uid {}) — awaiting parent decision",
                        req.source, req.package, req.id, uid
                    );
                    if let Some(a) = &self.audit {
                        a.record(AuditEvent::Submitted {
                            id: req.id,
                            uid,
                            source: req.source,
                            package: req.package.clone(),
                            reason: req.reason.clone(),
                        });
                    }
                    if let Some(ntfy) = &self.ntfy {
                        if let Err(e) = ntfy.push(&req).await {
                            eprintln!("guardiand: ntfy push failed for {}: {e:#} — held for local decision", req.id);
                        }
                    }
                    let (decision, via) = match tokio::time::timeout(self.decision_timeout, rx).await {
                        Ok(Ok((d, v))) => (d, v),
                        Ok(Err(_)) => (self.default_on_timeout, DecisionVia::Timeout),
                        Err(_) => {
                            self.queue.cancel(req.id);
                            eprintln!("guardiand: request {} timed out → {:?}", req.id, self.default_on_timeout);
                            (self.default_on_timeout, DecisionVia::Timeout)
                        }
                    };
                    if let Some(ntfy) = &self.ntfy {
                        ntfy.forget(&req.id);
                    }
                    if let Some(a) = &self.audit {
                        a.record(AuditEvent::Decided { id: req.id, decision, via });
                    }
                    send(&mut write_half, &ServerMessage::Decision { id: req.id, decision }).await?;
                    break; // one blocking request per submit connection
                }
                (false, _) => {
                    send(&mut write_half, &ServerMessage::Error {
                        message: "only submit is accepted on the submit socket".into(),
                    }).await?;
                }
            }
        }
        Ok(())
    }
}

async fn send<W>(w: &mut W, msg: &ServerMessage) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    w.write_all(s.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}
