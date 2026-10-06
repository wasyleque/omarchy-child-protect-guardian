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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use uuid::Uuid;

use crate::queue::Queue;
use crate::request::{Decision, InstallRequest, InstallSource};

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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    Decision { id: Uuid, decision: Decision },
    Pending { requests: Vec<InstallRequest> },
    Resolved { id: Uuid, ok: bool },
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
        tokio::select! {
            r = Arc::clone(&self).accept_loop(control, true) => r,
            r = Arc::clone(&self).accept_loop(submit, false) => r,
        }
    }

    async fn accept_loop(self: Arc<Self>, listener: UnixListener, is_control: bool) -> Result<()> {
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
                        continue;
                    }
                    None => {
                        eprintln!("guardiand: refused control connection (no peer credentials)");
                        continue;
                    }
                }
            }
            let uid = uid.unwrap_or(u32::MAX);
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = server.handle(stream, is_control, uid).await {
                    eprintln!("guardiand: connection error: {e:#}");
                }
            });
        }
    }

    async fn handle(&self, stream: UnixStream, is_control: bool, uid: u32) -> Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        let mut lines = BufReader::new(read_half).lines();

        while let Some(line) = lines.next_line().await? {
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
                    let ok = self.queue.resolve(id, decision);
                    send(&mut write_half, &ServerMessage::Resolved { id, ok }).await?;
                }
                (true, ClientMessage::Submit { .. }) => {
                    send(&mut write_half, &ServerMessage::Error {
                        message: "submit not accepted on the control socket".into(),
                    }).await?;
                }

                // ---- submit socket: submit only ----
                (false, ClientMessage::Submit { source, package, reason }) => {
                    let held = self.queue.try_submit(source, package, reason, uid, self.max_pending_per_uid);
                    let (req, rx) = match held {
                        Some(v) => v,
                        None => {
                            eprintln!("guardiand: refused submit from uid {uid} (per-uid cap reached) → deny");
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
                    if let Some(ntfy) = &self.ntfy {
                        if let Err(e) = ntfy.push(&req).await {
                            eprintln!("guardiand: ntfy push failed for {}: {e:#} — held for local decision", req.id);
                        }
                    }
                    let decision = match tokio::time::timeout(self.decision_timeout, rx).await {
                        Ok(Ok(d)) => d,
                        Ok(Err(_)) => self.default_on_timeout,
                        Err(_) => {
                            self.queue.cancel(req.id);
                            eprintln!("guardiand: request {} timed out → {:?}", req.id, self.default_on_timeout);
                            self.default_on_timeout
                        }
                    };
                    if let Some(ntfy) = &self.ntfy {
                        ntfy.forget(&req.id);
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
