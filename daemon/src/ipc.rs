//! Local IPC: a Unix-domain socket speaking newline-delimited JSON.
//!
//! Two kinds of client connect here:
//!   * the **interception hook** sends one [`ClientMessage::Submit`] and blocks on the
//!     connection until the daemon replies with a [`ServerMessage::Decision`];
//!   * the **control client** (`guardian-ctl`) sends [`ClientMessage::List`] /
//!     [`ClientMessage::Resolve`] and reads the matching reply.
//!
//! Stage 1 keeps the socket owner/group-only (mode 0660). Hardening (peer-credential
//! checks, splitting the hook vs. control surfaces) comes in a later stage.

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

/// A message from a client to the daemon.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientMessage {
    /// From the interception hook: register an install attempt and block for a decision.
    Submit {
        source: InstallSource,
        package: String,
        #[serde(default)]
        reason: Option<String>,
    },
    /// From the control client: list currently-held requests.
    List,
    /// From the control client: resolve a held request.
    Resolve { id: Uuid, decision: Decision },
}

/// A message from the daemon back to a client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Final answer for a `Submit` (also used when a request times out).
    Decision { id: Uuid, decision: Decision },
    /// Answer for a `List`.
    Pending { requests: Vec<InstallRequest> },
    /// Answer for a `Resolve`; `ok` is false if the id was unknown.
    Resolved { id: Uuid, ok: bool },
    /// A malformed or unprocessable request.
    Error { message: String },
}

/// The running IPC server and the policy it enforces for held requests.
pub struct Server {
    pub queue: Arc<Queue>,
    pub decision_timeout: Duration,
    pub default_on_timeout: Decision,
}

impl Server {
    /// Bind the socket and serve connections until an accept error occurs.
    pub async fn run(self: Arc<Self>, socket_path: &Path) -> Result<()> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        // Remove a stale socket left by a previous run.
        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding unix socket at {}", socket_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                socket_path,
                std::fs::Permissions::from_mode(0o660),
            );
        }
        eprintln!("guardiand: listening on {}", socket_path.display());

        loop {
            let (stream, _addr) = listener
                .accept()
                .await
                .context("accepting a connection")?;
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = server.handle(stream).await {
                    eprintln!("guardiand: connection error: {e:#}");
                }
            });
        }
    }

    async fn handle(&self, stream: UnixStream) -> Result<()> {
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
                    send(&mut write_half, &ServerMessage::Error {
                        message: format!("bad request: {e}"),
                    })
                    .await?;
                    continue;
                }
            };

            match msg {
                ClientMessage::List => {
                    let requests = self.queue.list();
                    send(&mut write_half, &ServerMessage::Pending { requests }).await?;
                }
                ClientMessage::Resolve { id, decision } => {
                    let ok = self.queue.resolve(id, decision);
                    send(&mut write_half, &ServerMessage::Resolved { id, ok }).await?;
                }
                ClientMessage::Submit {
                    source,
                    package,
                    reason,
                } => {
                    let (req, rx) = self.queue.submit(source, package, reason);
                    eprintln!(
                        "guardiand: HELD {:?} install '{}' (id {}) — awaiting parent decision",
                        req.source, req.package, req.id
                    );
                    let decision = match tokio::time::timeout(self.decision_timeout, rx).await {
                        Ok(Ok(d)) => d,
                        // Sender dropped without a decision: fall back to policy.
                        Ok(Err(_)) => self.default_on_timeout,
                        // Timed out: clean up and fall back to policy.
                        Err(_) => {
                            self.queue.cancel(req.id);
                            eprintln!(
                                "guardiand: request {} timed out → {:?}",
                                req.id, self.default_on_timeout
                            );
                            self.default_on_timeout
                        }
                    };
                    send(&mut write_half, &ServerMessage::Decision {
                        id: req.id,
                        decision,
                    })
                    .await?;
                    // A blocking hook connection is done after its single decision.
                    break;
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
