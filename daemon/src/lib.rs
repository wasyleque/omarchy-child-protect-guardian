//! Omarchy Child Protect Guardian — core library (Stage 1).
//!
//! Stage 1 scope: a root daemon (`guardiand`) that *holds* an install request and
//! blocks until a decision is made, plus a control client (`guardian-ctl`) that lists
//! held requests and resolves them. Later stages replace the local decision with a
//! remote, cryptographically-signed push-approval from the parent's phone, and add the
//! real pacman/flatpak interception hooks.

pub mod audit;
pub mod config;
pub mod crypto;
pub mod ipc;
pub mod ntfy;
pub mod queue;
pub mod request;
pub mod schedule;
