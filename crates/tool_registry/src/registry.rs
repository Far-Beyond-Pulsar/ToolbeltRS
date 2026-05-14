//! The central [`ToolRegistry`] — store, look up, and dispatch tools.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::{DynamicTool, DynamicToolBuilder};
use crate::tool::ChatTool;
use crate::{PluginToolRegistry, ToolContext, ToolDefinition};
use serde_json::{json, Value};

/// Central registry that maps tool names → `Arc<dyn ChatTool>`.
///
/// # Typical set-up
///
/// ```rust
/// use tool_registry::ToolRegistry;
/// # use tool_registry::{ChatTool, ToolContext};
/// # use serde_json::{json, Value};
/// # struct Ping; impl ChatTool for Ping {
/// #   fn name(&self) -> &'static str { "ping" }
/// #   fn description(&self) -> &'static str { "" }
/// #   fn parameters_schema(&self) -> Value { json!({"type":"object","properties":{}}) }
/// #   fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<Value> { Ok(json!({})) }
/// # }
/// let mut registry = ToolRegistry::new();
/// registry.register(std::sync::Arc::new(Ping));
/// ```
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ChatTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a single tool.  Overwrites any previous tool with the same name.
    pub fn register(&mut self, tool: Arc<dyn ChatTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Merge all tools from a [`PluginToolRegistry`] into this registry.
    ///
    /// Call once per plugin during application start-up.
    pub fn merge_plugin(&mut self, plugin: &PluginToolRegistry) {
        for tool in plugin.tools() {
            self.register(tool);
        }
    }

    /// Merge all tools provided by a [`ToolPlugin`](crate::ToolPlugin).
    pub fn add_plugin(&mut self, plugin: &dyn crate::ToolPlugin) {
        self.merge_plugin(&plugin.tool_registry());
    }

    /// Execute a tool by name.
    pub fn execute(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?;
        tool.execute(args, ctx)
    }

    /// Returns `true` if a tool with this name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// All registered tool names.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }

    /// All tool [`ToolDefinition`]s — suitable for building an LLM request payload.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> =
            self.tools.values().map(|t| t.definition()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Remove a tool by name.
    ///
    /// Returns `true` if a tool with that name was present and removed,
    /// `false` if no such tool existed.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }

    /// Register a tool defined inline as a closure, without implementing
    /// [`ChatTool`] manually.
    ///
    /// This is a shorthand for [`DynamicTool::builder`] +
    /// [`ToolRegistry::register`].  For more control (e.g. setting a category
    /// or a custom schema) use [`build_tool`](Self::build_tool) instead.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tool_registry::{ToolRegistry, ToolContext, tool_params};
    /// use serde_json::json;
    ///
    /// let mut registry = ToolRegistry::new();
    /// registry.register_fn(
    ///     "ping",
    ///     "Returns pong",
    ///     tool_params!(),
    ///     |_args, _ctx| Ok(json!({ "message": "pong" })),
    /// );
    /// let result = registry.execute("ping", json!({}), &ToolContext::default()).unwrap();
    /// assert_eq!(result["message"], "pong");
    /// ```
    pub fn register_fn<F>(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: Value,
        handler: F,
    ) where
        F: Fn(Value, &ToolContext) -> anyhow::Result<Value> + Send + Sync + 'static,
    {
        let tool = DynamicTool::builder(name)
            .description(description)
            .parameters(parameters_schema)
            .handler(handler)
            .build();
        self.register(Arc::new(tool));
    }

    /// Start building a [`DynamicTool`] that will be registered into this
    /// registry once [`DynamicToolBuilder::register_into`] is called.
    ///
    /// This is a convenience for the fluent builder pattern when you want to
    /// set a category or other fields not available through [`register_fn`](Self::register_fn).
    ///
    /// # Example
    ///
    /// ```rust
    /// use tool_registry::{ToolRegistry, ToolContext, tool_params};
    /// use serde_json::json;
    ///
    /// let mut registry = ToolRegistry::new();
    /// let tool = registry
    ///     .build_tool("add")
    ///     .description("Add two numbers")
    ///     .category("math")
    ///     .parameters(tool_params! { req "a": number = "First operand", req "b": number = "Second operand" })
    ///     .handler(|args, _ctx| {
    ///         let a = args["a"].as_f64().unwrap_or(0.0);
    ///         let b = args["b"].as_f64().unwrap_or(0.0);
    ///         Ok(json!({ "result": a + b }))
    ///     })
    ///     .build();
    /// registry.register(std::sync::Arc::new(tool));
    /// ```
    pub fn build_tool(&self, name: impl Into<String>) -> DynamicToolBuilder {
        DynamicTool::builder(name)
    }

    /// Serialise all tool definitions as a JSON array — the shape expected by
    /// OpenAI-compatible `/chat/completions` `tools` field.
    pub fn openai_tools_array(&self) -> Value {
        json!(self
            .definitions()
            .into_iter()
            .map(|d| json!({
                "type": "function",
                "function": {
                    "name": d.name,
                    "description": d.description,
                    "parameters": d.parameters_schema,
                }
            }))
            .collect::<Vec<_>>())
    }

    /// Build a compact human-readable tool list for inclusion in a system prompt.
    pub fn system_prompt_section(&self) -> String {
        let mut lines = vec!["Available tools:".to_string()];
        for def in self.definitions() {
            let params = def
                .parameters_schema
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|props| {
                    let required: Vec<&str> = def
                        .parameters_schema
                        .get("required")
                        .and_then(|r| r.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    props
                        .keys()
                        .map(|k| {
                            if required.contains(&k.as_str()) {
                                k.clone()
                            } else {
                                format!("[{k}]")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            let category = def
                .category
                .as_deref()
                .map(|c| format!(" [{c}]"))
                .unwrap_or_default();

            if params.is_empty() {
                lines.push(format!("  • {}(){} — {}", def.name, category, def.description));
            } else {
                lines.push(format!(
                    "  • {}({}){} — {}",
                    def.name, params, category, def.description
                ));
            }
        }
        lines.join("\n")
    }
}
