use std::collections::HashMap;
use std::path::PathBuf;

const DEFAULT_SHELL: &str = "/bin/bash";

#[derive(Debug, Clone)]
pub struct BackendSettings {
    pub shell: String,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    /// LOCAL CHANGE (mogeung): extra environment for the child. Upstream had
    /// no way to set any, and the child otherwise inherits the *window*
    /// process's environment — which, when the window was not started from a
    /// terminal, has no `TERM` at all. See `mogeung-ui`'s `term.rs`.
    pub env: HashMap<String, String>,
}

impl Default for BackendSettings {
    fn default() -> Self {
        Self {
            shell: DEFAULT_SHELL.to_string(),
            args: vec![],
            working_directory: None,
            env: HashMap::new(),
        }
    }
}
