//! mogeungd as a library, so the binary and the integration tests share one
//! definition of the daemon.

pub mod adapter;
pub mod api;
pub mod codex;
pub mod docscan;
pub mod git;
pub mod insight;
pub mod health;
pub mod machine;
pub mod notify;
pub mod runner;
pub mod server;
pub mod state;
pub mod store;
pub mod usage;
pub mod watcher;
