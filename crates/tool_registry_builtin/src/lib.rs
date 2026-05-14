//! Built-in tools for `tool_registry`.
//!
//! Enable with the optional `builtin` feature on `tool_registry` or depend on
//! this crate directly.  Add to a registry:
//!
//! ```rust,ignore
//! use tool_registry::ToolRegistry;
//! use tool_registry_builtin::register_builtins;
//!
//! let mut registry = ToolRegistry::new();
//! register_builtins(&mut registry);
//! ```

mod fetch_url;
mod web_search;

pub use fetch_url::FetchUrlTool;
pub use web_search::WebSearchTool;

use tool_registry::ToolRegistry;
use std::sync::Arc;

/// Register all built-in tools into the given [`ToolRegistry`].
pub fn register_builtins(registry: &mut ToolRegistry) {
    registry.register(Arc::new(WebSearchTool));
    registry.register(Arc::new(FetchUrlTool));
}
