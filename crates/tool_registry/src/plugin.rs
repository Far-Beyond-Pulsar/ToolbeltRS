//! [`ToolPlugin`] trait — the contract a plugin must implement to supply tools
//! to the central [`ToolRegistry`].
//!
//! # Minimal example
//!
//! ```rust,ignore
//! use tool_registry::{ToolPlugin, PluginToolRegistry};
//!
//! pub struct MyPlugin;
//!
//! impl ToolPlugin for MyPlugin {
//!     fn name(&self) -> &'static str { "my_plugin" }
//!
//!     fn tool_registry(&self) -> PluginToolRegistry {
//!         // Collect compile-time inventory from this plugin's module path.
//!         PluginToolRegistry::from_namespace(module_path!())
//!     }
//! }
//! ```
//!
//! Then at start-up:
//!
//! ```rust,ignore
//! registry.add_plugin(&MyPlugin);
//! ```

use crate::PluginToolRegistry;

/// Implement this trait on your plugin struct to participate in the tool registry.
///
/// The registry calls [`tool_registry`] once at start-up.  Return a
/// [`PluginToolRegistry`] assembled from compile-time `inventory` entries
/// (via [`PluginToolRegistry::from_namespace`]) or built manually.
pub trait ToolPlugin: Send + Sync {
    /// Unique plugin name — used for diagnostics only.
    fn name(&self) -> &'static str;

    /// Return all tools this plugin contributes.
    fn tool_registry(&self) -> PluginToolRegistry;
}
