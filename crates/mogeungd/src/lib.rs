//! mogeungd as a library, so the binary and the integration tests share one
//! definition of the daemon.

pub mod adapter;
pub mod api;
pub mod git;
pub mod health;
pub mod notify;
pub mod state;
pub mod store;
pub mod watcher;
pub mod web;
