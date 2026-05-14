//! The central [`ToolRegistry`] — store, look up, and dispatch tools.

use std::collections::HashMap;
use std::sync::Arc;

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
