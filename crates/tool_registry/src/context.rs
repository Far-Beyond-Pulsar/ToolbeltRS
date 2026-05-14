//! Caller-supplied context passed to every tool on execution.
//!
//! [`ToolContext`] is intentionally generic — store whatever your application
//! needs tools to access (workspace path, open-file callbacks, etc.).
//! For simple uses, `ToolContext::default()` is valid.

use std::path::PathBuf;
use std::sync::Arc;

/// Application-supplied execution context forwarded to every tool call.
///
/// All fields are optional so the type works out-of-the-box with `Default`.
/// Populate only the fields your tools actually need.
#[derive(Clone, Default)]
pub struct ToolContext {
    /// Root directory of the current workspace / project.
    pub workspace_root: Option<PathBuf>,

    /// The file (if any) the user is currently editing.
    pub current_file: Option<PathBuf>,

    /// Arbitrary key-value extras for application-specific data.
    ///
    /// Use this to pass domain objects that don't fit the typed fields without
    /// adding hard dependencies to this crate.
    pub extras: std::collections::HashMap<String, Arc<dyn std::any::Any + Send + Sync>>,
}

impl ToolContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience: set `workspace_root`.
    pub fn with_workspace(mut self, root: impl Into<PathBuf>) -> Self {
        self.workspace_root = Some(root.into());
        self
    }

    /// Convenience: set `current_file`.
    pub fn with_current_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.current_file = Some(file.into());
        self
    }

    /// Insert an arbitrary typed extra.
    pub fn insert_extra<T: Send + Sync + 'static>(&mut self, key: impl Into<String>, value: T) {
        self.extras.insert(key.into(), Arc::new(value));
    }

    /// Retrieve an arbitrary typed extra by key.
    pub fn get_extra<T: 'static>(&self, key: &str) -> Option<&T> {
        self.extras
            .get(key)
            .and_then(|v| v.downcast_ref::<T>())
    }
}
