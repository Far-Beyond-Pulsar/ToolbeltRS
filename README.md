<div align="center">

# ai-tool-registry

**A generic, engine-agnostic AI tool registry for Rust.**

Define tools once. Dispatch them anywhere. Ship plugins that contribute tools at compile time.

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

</div>

---

## Table of Contents

- [Overview](#overview)
- [Crates in this workspace](#crates-in-this-workspace)
- [Architecture](#architecture)
  - [Three-layer model](#three-layer-model)
  - [Data flow](#data-flow)
- [Getting started](#getting-started)
  - [Add to Cargo.toml](#add-to-cargotoml)
  - [Hello tool](#hello-tool)
- [The ChatTool trait](#the-chattool-trait)
  - [Required methods](#required-methods)
  - [Optional methods](#optional-methods)
  - [ToolDefinition](#tooldefinition)
- [ToolRegistry](#toolregistry)
  - [Registering tools](#registering-tools)
  - [Executing tools](#executing-tools)
  - [Querying tools](#querying-tools)
  - [LLM payload generation](#llm-payload-generation)
- [ToolContext](#toolcontext)
  - [Typed extras](#typed-extras)
- [The tool macro](#the-tool-macro)
  - [Basic usage](#basic-usage)
  - [Macro options](#macro-options)
  - [Parameter type mapping](#parameter-type-mapping)
  - [External documentation files](#external-documentation-files)
  - [Generated code](#generated-code)
- [The tool_params macro](#the-tool_params-macro)
- [Plugin system](#plugin-system)
  - [PluginToolRegistry](#plugintoolregistry)
  - [Implementing ToolPlugin](#implementing-toolplugin)
  - [Merging plugins at start-up](#merging-plugins-at-start-up)
  - [Compile-time inventory](#compile-time-inventory)
  - [Multiple plugins](#multiple-plugins)
- [Built-in tools](#built-in-tools)
  - [web_search](#web_search)
  - [fetch_url](#fetch_url)
- [LLM integration patterns](#llm-integration-patterns)
  - [OpenAI compatible APIs](#openai-compatible-apis)
  - [System prompt injection](#system-prompt-injection)
  - [Tool call dispatch loop](#tool-call-dispatch-loop)
- [Design decisions](#design-decisions)
- [Testing](#testing)
- [License](#license)

---

## Overview

`ai-tool-registry` solves a common problem in LLM-backed Rust applications: **how do you let different parts of your codebase — and third-party plugins — contribute callable tools to an AI agent without coupling them together?**

The answer is a three-layer system:

1. **Traits** — `ChatTool` is the unit of work. Any struct that implements it is a tool.
2. **Registry** — `ToolRegistry` is the runtime store. It holds `Arc<dyn ChatTool>` values and dispatches calls by name.
3. **Inventory** — The `#[tool]` macro emits link-time records via [`inventory`](https://docs.rs/inventory). `PluginToolRegistry::from_namespace` harvests them at start-up with zero boilerplate.

Plugins **never import the central registry**. They compile against `tool_registry` and emit inventory entries. The host binary collects them. This means plugins can be third-party crates — as long as they are linked in, their tools appear automatically.

---

## Crates in this workspace

| Crate | Path | Description |
|-------|------|-------------|
| `tool_registry` | [`crates/tool_registry`](crates/tool_registry) | Core crate — `ChatTool`, `ToolRegistry`, `ToolPlugin`, `PluginToolRegistry`, `tool_params!` |
| `tool_registry_macros` | [`crates/tool_registry_macros`](crates/tool_registry_macros) | Proc-macro crate — `#[tool]` attribute, JSON Schema derivation, inventory submission |
| `tool_registry_builtin` | [`crates/tool_registry_builtin`](crates/tool_registry_builtin) | Ready-made tools: `web_search`, `fetch_url` |

---

## Architecture

### Three-layer model

```
+------------------------------------------------------------------+
|                    Host Application                              |
|                                                                  |
|   ToolRegistry::new()                                            |
|   registry.add_plugin(&PluginA)    <- merges PluginA's tools   |
|   registry.add_plugin(&PluginB)    <- merges PluginB's tools   |
|   registry.execute("tool_name", args, &ctx)                     |
+------------------------------------------------------------------+
                        ^               ^
            implements  |               |  implements
                        |               |
        +---------------+-+   +---------+----------+
        |  PluginA crate  |   |   PluginB crate    |
        |                 |   |                    |
        |  #[tool]        |   |  impl ChatTool     |
        |  fn search(..)  |   |  for MySpecialTool |
        |                 |   |                    |
        |  PluginToolReg  |   |  PluginToolReg     |
        |  ::from_ns(..)  |   |  ::new() + .add()  |
        +-----------------+   +--------------------+
```

### Data flow

```
  LLM response
      |
      |  { "name": "web_search", "arguments": { "query": "rust async" } }
      v
  registry.execute("web_search", args, &ctx)
      |
      +- looks up Arc<dyn ChatTool> by name
      |
      +- tool.execute(args, ctx)
              |
              v
          Ok(json!({ "results": [...] }))
              |
              v
  Send back to LLM as tool result message
```

---

## Getting started

### Add to Cargo.toml

```toml
[dependencies]
tool_registry        = { git = "https://github.com/Far-Beyond-Pulsar/ai-tool-registry" }
tool_registry_macros = { git = "https://github.com/Far-Beyond-Pulsar/ai-tool-registry" }
serde_json           = "1"
anyhow               = "1"

# Optional: built-in web tools
tool_registry_builtin = { git = "https://github.com/Far-Beyond-Pulsar/ai-tool-registry" }
```

### Hello tool

```rust
use std::sync::Arc;
use serde_json::{json, Value};
use tool_registry::{ChatTool, ToolContext, ToolRegistry, tool_params};

struct EchoTool;

impl ChatTool for EchoTool {
    fn name(&self)        -> &'static str { "echo" }
    fn description(&self) -> &'static str { "Returns whatever text you send it." }
    fn parameters_schema(&self) -> Value {
        tool_params! { req "text": string = "The text to echo back" }
    }
    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let text = args["text"].as_str().unwrap_or("");
        Ok(json!({ "echoed": text }))
    }
}

fn main() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));

    let ctx    = ToolContext::default();
    let result = registry.execute("echo", json!({ "text": "hello" }), &ctx).unwrap();
    println!("{result}");  // {"echoed":"hello"}
}
```

---

## The ChatTool trait

```rust
pub trait ChatTool: Send + Sync {
    fn name(&self)              -> &'static str;
    fn description(&self)       -> &'static str;
    fn parameters_schema(&self) -> Value;
    fn category(&self)          -> Option<&'static str> { None }
    fn execute(&self, args: Value, ctx: &ToolContext) -> anyhow::Result<Value>;
    fn definition(&self)        -> ToolDefinition { /* auto-derived */ }
}
```

### Required methods

| Method | Purpose |
|--------|---------|
| `name` | Short snake_case identifier. The LLM calls tools by this exact name. Must be unique across the registry. |
| `description` | Natural-language description sent to the LLM. Good descriptions are the largest lever for tool-call accuracy. |
| `parameters_schema` | A JSON Schema `object` value describing accepted arguments. Use `tool_params!` or hand-write it. |
| `execute` | Called when the LLM (or test code) invokes the tool. Receives the JSON args and the caller's `ToolContext`. Must return `anyhow::Result<Value>`. |

### Optional methods

| Method | Default | Purpose |
|--------|---------|---------|
| `category` | `None` | Groups tools in the system-prompt section (e.g. `"web"`, `"math"`, `"file"`). |
| `definition` | auto-derived | Returns a serialisable `ToolDefinition`. Override only if you need custom serialization. |

### ToolDefinition

`ToolDefinition` is the serializable form of a tool's metadata:

```rust
pub struct ToolDefinition {
    pub name:              String,
    pub description:       String,
    pub parameters_schema: Value,
    pub category:          Option<String>,
}
```

It implements `serde::Serialize` / `Deserialize` so it can be forwarded over the wire, stored in a database, or sent to a proxy service.

---

## ToolRegistry

`ToolRegistry` is the central store. It holds `Arc<dyn ChatTool>` and dispatches calls by name in O(1).

### Registering tools

```rust
// Single tool
registry.register(Arc::new(MyTool));

// From a PluginToolRegistry
registry.merge_plugin(&plugin.tool_registry());

// From a ToolPlugin implementor
registry.add_plugin(&MyPlugin);
```

### Executing tools

```rust
let result: Value = registry.execute("tool_name", json_args, &ctx)?;
```

Returns `Err` if the tool is not registered or if the tool itself returns an error.

### Querying tools

```rust
// All registered names, sorted alphabetically
let names: Vec<&str> = registry.names();

// Membership check
let exists: bool = registry.contains("web_search");

// All definitions, sorted by name
let defs: Vec<ToolDefinition> = registry.definitions();
```

### LLM payload generation

```rust
// OpenAI-compatible "tools" array for POST /v1/chat/completions
let tools_json: Value = registry.openai_tools_array();

// Compact human-readable string for system prompt injection
let section: String = registry.system_prompt_section();
```

`openai_tools_array()` output shape:

```json
[
  {
    "type": "function",
    "function": {
      "name": "web_search",
      "description": "Search the web via DuckDuckGo.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "Search query string" }
        },
        "required": ["query"]
      }
    }
  }
]
```

---

## ToolContext

`ToolContext` is caller-supplied execution context forwarded to every tool call. It is not tied to any framework.

```rust
#[derive(Clone, Default)]
pub struct ToolContext {
    pub workspace_root: Option<PathBuf>,
    pub current_file:   Option<PathBuf>,
    pub extras: HashMap<String, Arc<dyn Any + Send + Sync>>,
}
```

Builder-style construction:

```rust
let ctx = ToolContext::new()
    .with_workspace("/home/user/my_project")
    .with_current_file("/home/user/my_project/src/main.rs");
```

### Typed extras

Store arbitrary application-specific data without adding hard dependencies to this crate:

```rust
// In the caller
ctx.insert_extra("db_pool", my_pool.clone());
ctx.insert_extra("config",  Arc::new(app_config));

// Inside a tool's execute()
if let Some(pool) = ctx.get_extra::<DbPool>("db_pool") {
    let row = pool.query_one("SELECT 1", &[]).await?;
}
```

The `extras` map uses `Arc<dyn Any + Send + Sync>` so values can be cheaply cloned across threads.

---

## The tool macro

Add `tool_registry_macros` to `[dependencies]` in your plugin crate:

```toml
[dependencies]
tool_registry        = { git = "…" }
tool_registry_macros = { git = "…" }
```

### Basic usage

```rust
use tool_registry_macros::tool;
use serde_json::Value;

/// Compute the square root of a number.
#[tool]
pub fn sqrt(value: f64) -> anyhow::Result<Value> {
    anyhow::ensure!(value >= 0.0, "sqrt of a negative number is undefined");
    Ok(serde_json::json!({ "result": value.sqrt() }))
}
```

The doc-comment becomes the tool description in the LLM's tool list.

### Macro options

```rust
#[tool(
    category   = "math",        // grouping shown in system-prompt output
    timeout_ms = 2000,          // execution-time hint stored in the definition JSON
    docs       = "docs/sqrt.md" // embed an external .md file via include_str!()
)]
```

All options are optional. `#[tool]` with no arguments is valid.

### Parameter type mapping

| Rust type | JSON Schema type | Notes |
|-----------|-----------------|-------|
| `String` / `&str` | `"string"` | |
| `i8`…`i128`, `u8`…`u128`, `isize`, `usize` | `"integer"` | |
| `f32`, `f64` | `"number"` | |
| `bool` | `"boolean"` | |
| `Option<T>` | type of inner `T` | **Not** added to `required` |
| anything else | `"object"` | For `serde::Deserialize` structs |

Parameters not wrapped in `Option<_>` are automatically added to the `required` array:

```rust
/// Search for files matching a pattern.
#[tool]
pub fn find_files(
    directory: String,          // required
    extension: Option<String>,  // optional — omitted from "required"
    max_depth: Option<u32>,     // optional — omitted from "required"
) -> anyhow::Result<Value> {
    // …
}
```

### External documentation files

```rust
#[tool(docs = "docs/ai/web_search.md")]
/// Search the web.
pub fn web_search(query: String) -> anyhow::Result<Value> { /* … */ }
```

The file is embedded at compile time via `include_str!()`, relative to the crate root.

### Generated code

For `fn my_tool(a: String, b: Option<i64>)` the macro emits five items:

```rust
// 1. Serialised definition constant
pub const TOOL_DEF_MY_TOOL: &str = r#"{"name":"my_tool","description":"…",...}"#;

// 2. Markdown doc constant
pub const TOOL_DOC_MY_TOOL: &str = "# `my_tool`\n…";

// 3. JSON-arg-extracting wrapper (the actual inventory handler fn)
pub fn my_tool_tool_wrapper(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let a: String      = serde_json::from_value(args.get("a").ok_or(…)?.clone())?;
    let b: Option<i64> = args.get("b").and_then(|v| serde_json::from_value(v.clone()).ok());
    my_tool(a, b)
}

// 4. Link-time inventory submission (runs before main)
inventory::submit! {
    tool_registry::InventoryEntry {
        namespace:       module_path!(),
        definition_json: TOOL_DEF_MY_TOOL,
        documentation:   TOOL_DOC_MY_TOOL,
        handler:         my_tool_tool_wrapper,
    }
}

// 5. The original function — completely unchanged
pub fn my_tool(a: String, b: Option<i64>) -> anyhow::Result<serde_json::Value> { /* … */ }
```

---

## The tool_params macro

`tool_params!` builds a JSON Schema `parameters` object inline without hand-writing JSON.

```rust
use tool_registry::tool_params;

// Zero parameters
let schema = tool_params!();

// Required and optional mix
let schema = tool_params! {
    req "query":          string  = "The search query",
    req "collection":     string  = "Collection to search in",
    opt "limit":          integer = "Max results (default 10)",
    opt "include_scores": boolean = "Return similarity scores",
};

assert_eq!(schema["required"][0], "query");
assert_eq!(schema["properties"]["limit"]["type"], "integer");
```

**Syntax:** `req|opt "name": json_type = "description"`.

Supported JSON types: `string`, `integer`, `number`, `boolean`, `object`, `array`.

`req` entries appear in `required`; `opt` entries do not.

---

## Plugin system

The plugin system lets independent crates contribute tools **without importing the central registry**. This is the key property that makes the library scale to engine-style applications with hundreds of plugins.

### PluginToolRegistry

A `PluginToolRegistry` bundles the `Arc<dyn ChatTool>` items from one plugin. Build it three ways:

**From `#[tool]` inventory (recommended):**

```rust
// Inside the plugin crate's init code
let registry = PluginToolRegistry::from_namespace(module_path!());
```

`module_path!()` resolves to the crate's module path at the call site (e.g. `"my_plugin::tools"`). Only entries whose `namespace` starts with that string are included — ensuring plugins cannot accidentally capture each other's tools.

**Manually (for hand-written `ChatTool` impls):**

```rust
let mut registry = PluginToolRegistry::new();
registry.add(Arc::new(ResizeTool));
registry.add(Arc::new(CropTool));
```

**From all linked crates (monolithic binaries):**

```rust
let registry = PluginToolRegistry::from_all_inventory();
```

### Implementing ToolPlugin

```rust
use tool_registry::{PluginToolRegistry, ToolPlugin};

pub struct ImagePlugin;

impl ToolPlugin for ImagePlugin {
    fn name(&self) -> &'static str { "image_plugin" }

    fn tool_registry(&self) -> PluginToolRegistry {
        PluginToolRegistry::from_namespace(module_path!())
    }
}
```

### Merging plugins at start-up

```rust
let mut registry = ToolRegistry::new();

registry.add_plugin(&ImagePlugin);
registry.add_plugin(&AudioPlugin);
registry.merge_plugin(&build_my_plugin_registry());
```

### Compile-time inventory

```
Plugin crate A                   Plugin crate B
  #[tool] fn search(..)            #[tool] fn resize(..)
      |                                |
      | inventory::submit!            | inventory::submit!
      v                                v
  +---------- linker section: InventoryEntry records ----------+
  | { ns: "plugin_a::..", def: "..", handler: search_wrap }   |
  | { ns: "plugin_b::..", def: "..", handler: resize_wrap }   |
  +------------------------------------------------------------+
                      |   program start (before main)
                      v
        PluginToolRegistry::from_namespace("plugin_a")
                      |
                 [ search ]   <- only plugin_a's entry matched
```

Properties:
- **Zero runtime cost** after start-up — dispatches through a `HashMap`.
- **No registration calls** — linking the plugin crate is sufficient.
- **Thread-safe** — only `&'static` data and function pointers; no locks.

### Multiple plugins

```rust
let plugins: Vec<Box<dyn ToolPlugin>> = vec![
    Box::new(FileSystemPlugin),
    Box::new(DatabasePlugin),
    Box::new(WebPlugin),
];

let mut registry = ToolRegistry::new();
for p in &plugins {
    registry.add_plugin(p.as_ref());
}

println!("{} tools loaded", registry.names().len());
```

---

## Built-in tools

```toml
[dependencies]
tool_registry_builtin = { git = "https://github.com/Far-Beyond-Pulsar/ai-tool-registry" }
```

```rust
use tool_registry::ToolRegistry;
use tool_registry_builtin::register_builtins;

let mut registry = ToolRegistry::new();
register_builtins(&mut registry);
```

Or register individually:

```rust
use std::sync::Arc;
use tool_registry_builtin::{FetchUrlTool, WebSearchTool};

registry.register(Arc::new(WebSearchTool));
registry.register(Arc::new(FetchUrlTool));
```

### web_search

| Field | Value |
|-------|-------|
| Name | `web_search` |
| Category | `web` |
| Description | Search the web via DuckDuckGo HTML. Returns up to 10 results. |

Parameters:

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `query` | string | yes | The search query |

Example response:

```json
{
  "ok": true,
  "query": "rust async runtime",
  "result_count": 3,
  "results": [
    {
      "title": "Tokio - An async Rust runtime",
      "summary": "Tokio is an event-driven, non-blocking I/O platform...",
      "url": "https://tokio.rs"
    }
  ]
}
```

### fetch_url

| Field | Value |
|-------|-------|
| Name | `fetch_url` |
| Category | `web` |
| Description | Fetch a URL and return its text content with HTML stripped. Truncated to ~8 000 chars. |

Parameters:

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `url` | string | yes | Full URL starting with `http://` or `https://` |
| `timeout_seconds` | integer | no | Timeout 1–30 seconds (default 10) |

---

## LLM integration patterns

### OpenAI compatible APIs

```rust
let request_body = serde_json::json!({
    "model": "gpt-4o",
    "messages": [
        { "role": "system", "content": "You are a helpful assistant." },
        { "role": "user",   "content": "Search for recent Rust news." },
    ],
    "tools":       registry.openai_tools_array(),
    "tool_choice": "auto",
});
```

### System prompt injection

```rust
let system_prompt = format!(
    "You are a helpful assistant.\n\n{}\n\nAlways prefer tools over guessing.",
    registry.system_prompt_section()
);
```

Sample output:

```
Available tools:
  • fetch_url(url, [timeout_seconds]) [web] — Fetch and return the text content of a URL...
  • web_search(query) [web] — Search the web via DuckDuckGo...
```

### Tool call dispatch loop

```rust
loop {
    let response = llm.complete(&messages, &registry.openai_tools_array()).await?;

    if response.finish_reason == "tool_calls" {
        for call in response.tool_calls {
            let args   = serde_json::from_str(&call.function.arguments)?;
            let result = registry.execute(&call.function.name, args, &ctx)?;
            messages.push(serde_json::json!({
                "role":         "tool",
                "tool_call_id": call.id,
                "content":      result.to_string(),
            }));
        }
        // Loop: send tool results back to LLM
    } else {
        println!("{}", response.content.unwrap_or_default());
        break;
    }
}
```

---

## Design decisions

**Why `Arc<dyn ChatTool>` instead of `Box`?**
Tools are often shared across threads and may be cloned into per-request registries (e.g. filtered by caller permissions). `Arc` makes this zero-copy.

**Why `inventory` instead of a global `Mutex<Vec>`?**
`inventory` collects static records at link time with no runtime synchronization. Plugins never need to call a registration function — linking the crate is sufficient. Same technique as `linkme`, `ctor`, `distributed_slice`.

**Why is `ToolContext` a concrete struct, not a trait?**
A trait-based context would require generic bounds on `ChatTool::execute`, breaking object safety and preventing `Arc<dyn ChatTool>`. The `extras` map provides type-safe plugin access without those constraints.

**Why `anyhow::Result<Value>` return type?**
LLM results must be JSON — requiring `Value` makes this explicit at the signature level. `anyhow` was chosen over a custom error type to keep plugin crates lean.

**Why does name collision favour the last registration?**
`register` overwrites existing entries. This lets the host override built-in tools with custom implementations without a separate override API.

---

## Testing

```bash
# Full workspace
cargo test --workspace

# Per crate
cargo test -p tool_registry
cargo test -p tool_registry_macros
cargo test -p tool_registry_builtin

# Doc-tests only
cargo test --doc -p tool_registry

# With output
cargo test --workspace -- --nocapture
```

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
