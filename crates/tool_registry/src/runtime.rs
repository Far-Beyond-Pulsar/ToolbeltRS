//! Runtime-defined tools via closures.
//!
//! [`DynamicTool`] lets you register a tool at runtime without implementing the
//! [`ChatTool`] trait manually on a new struct.  This is useful for:
//! - Tools whose names or descriptions are loaded from configuration files or a database.
//! - Scripted or plugin environments that generate tools from data at startup.
//! - Testing scenarios requiring quick one-off tools without boilerplate.
//!
//! # Example
//!
//! ```rust
//! use tool_registry::{DynamicTool, ToolRegistry, ToolContext, tool_params};
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! let tool = DynamicTool::builder("greet")
//!     .description("Return a greeting for the given name")
//!     .category("util")
//!     .parameters(tool_params! { req "name": string = "Name to greet" })
//!     .handler(|args, _ctx| {
//!         let name = args["name"].as_str().unwrap_or("world");
//!         Ok(json!({ "greeting": format!("Hello, {name}!") }))
//!     })
//!     .build();
//!
//! let mut registry = ToolRegistry::new();
//! registry.register(Arc::new(tool));
//!
//! let result = registry
//!     .execute("greet", json!({ "name": "Alice" }), &ToolContext::default())
//!     .unwrap();
//! assert_eq!(result["greeting"], "Hello, Alice!");
//! ```

use std::sync::Arc;

use serde_json::{json, Value};

use crate::{ChatTool, ToolContext};

type HandlerFn = Arc<dyn Fn(Value, &ToolContext) -> anyhow::Result<Value> + Send + Sync>;

// ─────────────────────────────────────────────────────────────────────────────
// DynamicTool
// ─────────────────────────────────────────────────────────────────────────────

/// A tool defined entirely at runtime via a closure.
///
/// Use [`DynamicTool::builder`] to construct one, then register it with
/// [`ToolRegistry::register`](crate::ToolRegistry::register).
pub struct DynamicTool {
    name: String,
    description: String,
    category: Option<String>,
    parameters_schema: Value,
    handler: HandlerFn,
}

impl ChatTool for DynamicTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    fn parameters_schema(&self) -> Value {
        self.parameters_schema.clone()
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> anyhow::Result<Value> {
        (self.handler)(args, ctx)
    }
}

impl DynamicTool {
    /// Start building a [`DynamicTool`] with the given name.
    ///
    /// Call methods on the returned [`DynamicToolBuilder`] to configure the
    /// tool, then call [`build`](DynamicToolBuilder::build) to finalise it.
    pub fn builder(name: impl Into<String>) -> DynamicToolBuilder {
        DynamicToolBuilder::new(name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DynamicToolBuilder
// ─────────────────────────────────────────────────────────────────────────────

/// Fluent builder for [`DynamicTool`].
///
/// Obtain one via [`DynamicTool::builder`].
pub struct DynamicToolBuilder {
    name: String,
    description: String,
    category: Option<String>,
    parameters_schema: Value,
    handler: Option<HandlerFn>,
}

impl DynamicToolBuilder {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            category: None,
            parameters_schema: json!({ "type": "object", "properties": {} }),
            handler: None,
        }
    }

    /// Set the human-readable description forwarded to the LLM.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Assign an optional grouping category shown in the system-prompt section.
    pub fn category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }

    /// Set the JSON Schema describing this tool's accepted parameters.
    ///
    /// Use [`tool_params!`](crate::tool_params) to build the schema inline, or
    /// supply a hand-written [`serde_json::Value`].
    pub fn parameters(mut self, schema: Value) -> Self {
        self.parameters_schema = schema;
        self
    }

    /// Provide the execution handler as a closure.
    ///
    /// The closure receives the parsed JSON arguments and the caller's
    /// [`ToolContext`].  It must be `Send + Sync + 'static`.
    pub fn handler<F>(mut self, f: F) -> Self
    where
        F: Fn(Value, &ToolContext) -> anyhow::Result<Value> + Send + Sync + 'static,
    {
        self.handler = Some(Arc::new(f));
        self
    }

    /// Consume the builder and produce a [`DynamicTool`].
    ///
    /// # Panics
    ///
    /// Panics if no handler was provided via [`handler`](Self::handler).
    pub fn build(self) -> DynamicTool {
        DynamicTool {
            name: self.name,
            description: self.description,
            category: self.category,
            parameters_schema: self.parameters_schema,
            handler: self
                .handler
                .expect("DynamicTool requires a handler — call .handler(|args, ctx| { … })"),
        }
    }
}
