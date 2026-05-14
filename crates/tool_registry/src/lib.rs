//! `tool_registry` — generic AI tool registry
//!
//! # Quick start
//!
//! ```rust
//! use tool_registry::{ChatTool, ToolContext, ToolRegistry};
//! use serde_json::{json, Value};
//!
//! struct PingTool;
//! impl ChatTool for PingTool {
//!     fn name(&self)        -> &'static str { "ping" }
//!     fn description(&self) -> &'static str { "Returns pong" }
//!     fn parameters_schema(&self) -> Value  { json!({"type":"object","properties":{}}) }
//!     fn execute(&self, _args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
//!         Ok(json!({"message": "pong"}))
//!     }
//! }
//!
//! let mut registry = ToolRegistry::new();
//! registry.register(std::sync::Arc::new(PingTool));
//! let result = registry.execute("ping", json!({}), &ToolContext::default()).unwrap();
//! assert_eq!(result["message"], "pong");
//! ```
//!
//! # Plugin system
//!
//! Plugins implement [`ToolPlugin`] and supply a pre-built [`PluginToolRegistry`]
//! (typically assembled at compile time via `#[tool]` + `inventory`).  Merge them
//! into the central registry at start-up:
//!
//! ```ignore
//! let mut registry = ToolRegistry::new();
//! registry.merge_plugin(&MyPlugin::tool_registry());
//! ```

pub mod context;
pub mod inventory_support;
#[macro_use]
pub mod macros;
pub mod plugin;
pub mod registry;
pub mod tool;

pub use context::ToolContext;
pub use inventory_support::{InventoryEntry, PluginToolRegistry};
pub use plugin::ToolPlugin;
pub use registry::ToolRegistry;
pub use tool::{ChatTool, ToolDefinition};

// Re-export inventory so plugin crates can do `tool_registry::inventory::submit!`
pub use inventory;
