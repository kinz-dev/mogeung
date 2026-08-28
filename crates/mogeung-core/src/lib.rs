//! Shared types for mogeung.
//!
//! This crate is the contract between the daemon (`mogeungd`) and any client.
//! It deliberately has no async, no I/O and no heavy dependencies, so that a
//! second client (a thin web UI, a CLI) can be built against it cheaply.

pub mod attention;
pub mod change;
pub mod config;
pub mod docs;
pub mod health;
pub mod insight;
pub mod kit;
pub mod llmproxy;
pub mod model;
pub mod pricing;
pub mod review;
pub mod run;
pub mod session;
pub mod transcript;
pub mod usage;
pub mod verify;
pub mod wire;

pub use attention::{AttentionItem, AttentionReason};
pub use change::{Change, FileChange, Hunk, RiskFlag, RiskLevel};
pub use health::{Alert, Health, LineClass};
pub use review::{BlastRadius, ReviewDebt};
pub use run::{Corroboration, Origin, Run, RunConfig, RunLine, RunState};
pub use session::{LiveStatus, Session, SessionId};
pub use transcript::{EventKind, TranscriptEvent};
pub use wire::{ClientMsg, ServerMsg};
