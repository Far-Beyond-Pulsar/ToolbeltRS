//! Example: define a plugin with `#[tool]` and merge into the central registry.
//!
//! Run with: `cargo run --example plugin_tools`

use std::sync::Arc;
use serde_json::{json, Value};
use tool_registry::{ChatTool, PluginToolRegistry, ToolContext, ToolPlugin, ToolRegistry, tool_params};

// ── A hand-written tool (no macro) ──────────────────────────────────────────

struct PingTool;

impl ChatTool for PingTool {
    fn name(&self)        -> &'static str { "ping" }
    fn description(&self) -> &'static str { "Returns pong." }
    fn parameters_schema(&self) -> Value  { tool_params!() }
    fn execute(&self, _args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        Ok(json!({ "message": "pong" }))
    }
}

// ── A plugin that bundles several tools ─────────────────────────────────────

struct MathPlugin;

impl ToolPlugin for MathPlugin {
    fn name(&self) -> &'static str { "math_plugin" }

    fn tool_registry(&self) -> PluginToolRegistry {
        // In a real plugin crate, you would use:
        //   PluginToolRegistry::from_namespace(module_path!())
        // to collect all #[tool]-annotated functions compiled into this crate.
        // Here we build manually to keep the example self-contained.
        let mut r = PluginToolRegistry::new();
        r.add(Arc::new(AddTool));
        r
    }
}

struct AddTool;
impl ChatTool for AddTool {
    fn name(&self)        -> &'static str { "add" }
    fn description(&self) -> &'static str { "Add two integers." }
    fn parameters_schema(&self) -> Value {
        tool_params! {
            req "a": integer = "First operand",
            req "b": integer = "Second operand",
        }
    }
    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let a = args["a"].as_i64().ok_or_else(|| anyhow::anyhow!("a must be an integer"))?;
        let b = args["b"].as_i64().ok_or_else(|| anyhow::anyhow!("b must be an integer"))?;
        Ok(json!({ "result": a + b }))
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let ctx = ToolContext::default();

    // Start with a central registry containing the ping tool.
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool));

    // Merge the plugin's tools at runtime.
    registry.add_plugin(&MathPlugin);

    println!("Registered tools: {:?}\n", registry.names());
    println!("{}\n", registry.system_prompt_section());

    let ping_result = registry.execute("ping", json!({}), &ctx).unwrap();
    println!("ping → {ping_result}");

    let add_result = registry.execute("add", json!({ "a": 3, "b": 7 }), &ctx).unwrap();
    println!("add(3,7) → {add_result}");

    let openai_payload = registry.openai_tools_array();
    println!("\nOpenAI tools array:\n{}", serde_json::to_string_pretty(&openai_payload).unwrap());
}
