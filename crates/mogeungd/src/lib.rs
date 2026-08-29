//! mogeungd as a library, so the binary and the integration tests share one
//! definition of the daemon.

pub mod adapter;
pub mod api;
pub mod codex;
pub mod complete;
pub mod detect;
pub mod discovery;
pub mod docscan;
pub mod embed;
pub mod git;
pub mod guide;
pub mod insight;
pub mod kit;
pub mod llmproxy;
pub mod health;
pub mod machine;
pub mod model;
pub mod notes;
pub mod notify;
pub mod qwen;
pub mod run;
pub mod runconfig;
pub mod runner;
pub mod send;
pub mod server;
pub mod state;
pub mod store;
pub mod usage;
pub mod watcher;
pub mod why;
pub mod workspace;
