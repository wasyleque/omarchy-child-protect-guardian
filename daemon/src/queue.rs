//! In-memory store of install requests that are held awaiting a decision.
//!
//! Each held request keeps a [`oneshot::Sender`] so that whichever connection is
//! blocking on the decision (the interception hook) is woken the instant a decision
//! arrives from the control client (Stage 1) or the remote approval layer (later).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot;
use uuid::Uuid;

use crate::request::{Decision, InstallRequest, InstallSource};

struct Pending {
    request: InstallRequest,
    responder: oneshot::Sender<Decision>,
}

/// Thread-safe store of currently-held requests.
///
/// The critical sections only touch the map (never `.await` while holding the lock),
/// so a plain `std::sync::Mutex` is correct and cheaper than an async mutex here.
#[derive(Default)]
pub struct Queue {
    pending: Mutex<HashMap<Uuid, Pending>>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new held request. Returns the created request (for logging) and a
    /// receiver that resolves when a decision is made (or `Err` if the entry is
    /// dropped, e.g. on timeout cleanup).
    pub fn submit(
        &self,
        source: InstallSource,
        package: String,
        reason: Option<String>,
    ) -> (InstallRequest, oneshot::Receiver<Decision>) {
        let id = Uuid::new_v4();
        let requested_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let request = InstallRequest {
            id,
            source,
            package,
            reason,
            requested_at,
        };
        let (tx, rx) = oneshot::channel();
        let mut map = self.pending.lock().unwrap();
        map.insert(
            id,
            Pending {
                request: request.clone(),
                responder: tx,
            },
        );
        (request, rx)
    }

    /// Snapshot of all currently-held requests, oldest first.
    pub fn list(&self) -> Vec<InstallRequest> {
        let map = self.pending.lock().unwrap();
        let mut v: Vec<InstallRequest> = map.values().map(|p| p.request.clone()).collect();
        v.sort_by_key(|r| r.requested_at);
        v
    }

    /// Resolve a held request with a decision. Returns `true` if the id existed.
    pub fn resolve(&self, id: Uuid, decision: Decision) -> bool {
        let mut map = self.pending.lock().unwrap();
        if let Some(p) = map.remove(&id) {
            // Ignore a send error: the waiter may have already timed out and gone away.
            let _ = p.responder.send(decision);
            true
        } else {
            false
        }
    }

    /// Drop a held request without notifying a waiter (used for timeout cleanup).
    pub fn cancel(&self, id: Uuid) {
        let mut map = self.pending.lock().unwrap();
        map.remove(&id);
    }

    /// Number of currently-held requests.
    pub fn len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_wakes_waiter_with_decision() {
        let q = Queue::new();
        let (req, rx) = q.submit(InstallSource::Pacman, "firefox".into(), None);
        assert_eq!(q.len(), 1);
        assert!(q.resolve(req.id, Decision::Allow));
        assert_eq!(rx.await.unwrap(), Decision::Allow);
        assert!(q.is_empty(), "resolved request must be removed");
    }

    #[test]
    fn resolve_unknown_id_is_false() {
        let q = Queue::new();
        assert!(!q.resolve(Uuid::new_v4(), Decision::Deny));
    }

    #[tokio::test]
    async fn cancel_drops_sender_so_waiter_sees_err() {
        let q = Queue::new();
        let (req, rx) = q.submit(InstallSource::Flatpak, "org.gimp.GIMP".into(), None);
        q.cancel(req.id);
        assert!(rx.await.is_err(), "cancelled request's receiver must error");
    }
}
