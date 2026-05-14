# ai-tool-registry

A **generic, engine-agnostic AI tool registry** for Rust.

Define tools with a `#[tool]` proc-macro or the `ChatTool` trait, register them in a central `ToolRegistry`, and dispatch LLM tool-calls at runtime.  Plugins provide their own **compile-time macro-generated registries** that are merged into the central registry at start-up.

---

## Crates

| Crate | Description |
|-------|-------------|
| `tool_registry` | Core — `ChatTool` trait, `ToolRegistry`, `ToolPlugin`, inventory support |
| `tool_registry_macros` | `#[tool]` proc-macro — JSON Schema derivation, inventory submission |
| `tool_registry_builtin` | Ready-made tools: `web_search`, `fetch_url` |

---

## Quick start

```toml
[dependencies]
tool_registry        = { path = "crates/tool_registry" }
tool_registry_macros = { path = "crates/tool_registry_macros" }
serde_json           = "1"
anyhow               = "1"
```

```rust
use tool_registry::{ChatTool, ToolContext, ToolRegistry};
use serde_json::{json, Value};
use std::sync::Arc;

struct PingTool;

impl ChatTool for PingTool {
    fn name(&self)        -> &'static str { "ping" }
    fn description(&self) -> &'static str { "Returns pong." }
    fn parameters_schema(&self) -> Value  { tool_registry::tool_params!() }
    fn execute(&self, _args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        Ok(json!({ "message": "pong" }))
    }
}

fn main() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(PingTool));

    let result = registry.execute("ping", json!({}), &ToolContext::default()).unwrap();
    println!("{result}"); // {"message":"pong"}
}
```

---

## `#[tool]` macro

```rust
use tool_registry_macros::tool;
use serde_json::Value;

#[tool(category = "math")]
/// Multiply two numbers together.
pub fn multiply(a: f64, b: f64) -> anyhow::Result<Value> {
    Ok(serde_json::json!({ "result": a * b }))
}
```

The macro:
1. Derives a JSON Schema from the function signature.
2. Generates a JSON-argument-extracting wrapper.
3. Auto-generates Markdown documentation (or embeds an external `.md` via `docs = "path"`).
4. Submits an `InventoryEntry` at link time via `inventory::submit!`.

Collect everything from a module namespace at start-up:

```rust
use tool_registry::{PluginToolRegistry, ToolRegistry};

let mut registry = ToolRegistry::new();
let plugin_tools = PluginToolRegistry::from_namespace(module_path!());
registry.merge_plugin(&plugin_tools);
```

---

## Plugin system

Plugins implement the `ToolPlugin` trait and supply a pre-built `PluginToolRegistry`.

```rust
use tool_registry::{PluginToolRegistry, ToolPlugin};

pub struct MyPlugin;

impl ToolPlugin for MyPlugin {
    fn name(&self) -> &'static str { "my_plugin" }

    fn tool_registry(&self) -> PluginToolRegistry {
        PluginToolRegistry::from_namespace(module_path!())
    }
}
```

Register at start-up:

```rust
registry.add_plugin(&MyPlugin);
```

---

## `tool_params!` macro

A declarative helper for writing parameter schemas inline:

```rust
use tool_registry::tool_params;

let schema = tool_params! {
    req "query":   string  = "Search query",
    opt "limit":   integer = "Maximum results",
};
```

---

## Built-in tools

```toml
[dependencies]
tool_registry_builtin = { path = "crates/tool_registry_builtin" }
```

```rust
use tool_registry::ToolRegistry;
use tool_registry_builtin::register_builtins;

let mut registry = ToolRegistry::new();
register_builtins(&mut registry);
```

Tools included:

| Name | Description |
|------|-------------|
| `web_search` | DuckDuckGo search, up to 10 results |
| `fetch_url` | Fetch URL and strip HTML, truncated to 8 000 chars |

---

## License

MIT OR Apache-2.0
