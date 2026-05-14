//! Integration tests for `tool_registry` core crate.

use std::sync::{Arc, Mutex};
use serde_json::{json, Value};
use tool_registry::{ChatTool, PluginToolRegistry, ToolContext, ToolDefinition, ToolPlugin, ToolRegistry, tool_params};

// ─────────────────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────────────────

struct EchoTool;
impl ChatTool for EchoTool {
    fn name(&self)        -> &'static str { "echo" }
    fn description(&self) -> &'static str { "Echoes the input text." }
    fn parameters_schema(&self) -> Value  { tool_params! { req "text": string = "Text to echo" } }
    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let text = args["text"].as_str().unwrap_or("").to_string();
        Ok(json!({ "echoed": text }))
    }
}

struct AddTool;
impl ChatTool for AddTool {
    fn name(&self)        -> &'static str { "add" }
    fn description(&self) -> &'static str { "Adds two integers." }
    fn category(&self)    -> Option<&'static str> { Some("math") }
    fn parameters_schema(&self) -> Value {
        tool_params! {
            req "a": integer = "First operand",
            req "b": integer = "Second operand",
        }
    }
    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let a = args["a"].as_i64().ok_or_else(|| anyhow::anyhow!("a must be integer"))?;
        let b = args["b"].as_i64().ok_or_else(|| anyhow::anyhow!("b must be integer"))?;
        Ok(json!({ "result": a + b }))
    }
}

struct FailTool;
impl ChatTool for FailTool {
    fn name(&self)        -> &'static str { "fail" }
    fn description(&self) -> &'static str { "Always fails." }
    fn parameters_schema(&self) -> Value  { tool_params!() }
    fn execute(&self, _args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        anyhow::bail!("intentional failure")
    }
}

/// Tool that mutates a shared counter to verify it was actually called.
struct CounterTool {
    count: Arc<Mutex<u32>>,
}
impl ChatTool for CounterTool {
    fn name(&self)        -> &'static str { "counter" }
    fn description(&self) -> &'static str { "Increments a shared counter." }
    fn parameters_schema(&self) -> Value  { tool_params!() }
    fn execute(&self, _args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let mut c = self.count.lock().unwrap();
        *c += 1;
        Ok(json!({ "count": *c }))
    }
}

fn registry_with_defaults() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(EchoTool));
    r.register(Arc::new(AddTool));
    r.register(Arc::new(FailTool));
    r
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRegistry — registration
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn registry_starts_empty() {
    let r = ToolRegistry::new();
    assert!(r.names().is_empty());
}

#[test]
fn register_single_tool() {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(EchoTool));
    assert!(r.contains("echo"));
    assert_eq!(r.names(), vec!["echo"]);
}

#[test]
fn register_multiple_tools() {
    let r = registry_with_defaults();
    let names = r.names();
    assert!(names.contains(&"echo"));
    assert!(names.contains(&"add"));
    assert!(names.contains(&"fail"));
    assert_eq!(names.len(), 3);
}

#[test]
fn names_are_sorted() {
    let r = registry_with_defaults();
    let names = r.names();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn register_overwrites_same_name() {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(EchoTool));
    // Register a different tool under the same name
    struct EchoTool2;
    impl ChatTool for EchoTool2 {
        fn name(&self) -> &'static str { "echo" }
        fn description(&self) -> &'static str { "overwritten" }
        fn parameters_schema(&self) -> Value { tool_params!() }
        fn execute(&self, _a: Value, _c: &ToolContext) -> anyhow::Result<Value> {
            Ok(json!({ "v": 2 }))
        }
    }
    r.register(Arc::new(EchoTool2));
    // Still only one entry
    assert_eq!(r.names().len(), 1);
    // But it's the new one
    let result = r.execute("echo", json!({}), &ToolContext::default()).unwrap();
    assert_eq!(result["v"], 2);
}

#[test]
fn contains_returns_false_for_unknown() {
    let r = registry_with_defaults();
    assert!(!r.contains("nonexistent_tool_xyz"));
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRegistry — execution
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn execute_echo_tool() {
    let r   = registry_with_defaults();
    let ctx = ToolContext::default();
    let res = r.execute("echo", json!({ "text": "hello world" }), &ctx).unwrap();
    assert_eq!(res["echoed"], "hello world");
}

#[test]
fn execute_add_tool() {
    let r   = registry_with_defaults();
    let ctx = ToolContext::default();
    let res = r.execute("add", json!({ "a": 3, "b": 7 }), &ctx).unwrap();
    assert_eq!(res["result"], 10);
}

#[test]
fn execute_add_negative_numbers() {
    let r   = registry_with_defaults();
    let ctx = ToolContext::default();
    let res = r.execute("add", json!({ "a": -5, "b": 3 }), &ctx).unwrap();
    assert_eq!(res["result"], -2);
}

#[test]
fn execute_returns_err_for_unknown_tool() {
    let r   = registry_with_defaults();
    let ctx = ToolContext::default();
    let err = r.execute("no_such_tool", json!({}), &ctx);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("no_such_tool"));
}

#[test]
fn execute_propagates_tool_error() {
    let r   = registry_with_defaults();
    let ctx = ToolContext::default();
    let err = r.execute("fail", json!({}), &ctx);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("intentional failure"));
}

#[test]
fn execute_actually_calls_tool() {
    let counter = Arc::new(Mutex::new(0u32));
    let tool    = CounterTool { count: Arc::clone(&counter) };
    let mut r   = ToolRegistry::new();
    r.register(Arc::new(tool));

    let ctx = ToolContext::default();
    r.execute("counter", json!({}), &ctx).unwrap();
    r.execute("counter", json!({}), &ctx).unwrap();
    r.execute("counter", json!({}), &ctx).unwrap();

    assert_eq!(*counter.lock().unwrap(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRegistry — definitions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn definitions_returns_one_per_tool() {
    let r = registry_with_defaults();
    assert_eq!(r.definitions().len(), 3);
}

#[test]
fn definitions_are_sorted_by_name() {
    let r    = registry_with_defaults();
    let defs = r.definitions();
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn definition_category_propagates() {
    let r    = registry_with_defaults();
    let defs = r.definitions();
    let add  = defs.iter().find(|d| d.name == "add").unwrap();
    assert_eq!(add.category.as_deref(), Some("math"));
}

#[test]
fn definition_no_category_is_none() {
    let r    = registry_with_defaults();
    let defs = r.definitions();
    let echo = defs.iter().find(|d| d.name == "echo").unwrap();
    assert!(echo.category.is_none());
}

#[test]
fn tool_definition_serialises_to_json() {
    let def = ToolDefinition {
        name: "test".to_string(),
        description: "A test tool".to_string(),
        parameters_schema: json!({"type":"object","properties":{}}),
        category: Some("test_cat".to_string()),
    };
    let s = serde_json::to_string(&def).unwrap();
    assert!(s.contains("\"name\":\"test\""));
    assert!(s.contains("\"category\":\"test_cat\""));
}

#[test]
fn tool_definition_roundtrips_through_json() {
    let original = ToolDefinition {
        name: "round".to_string(),
        description: "roundtrip".to_string(),
        parameters_schema: json!({"type":"object","properties":{"x":{"type":"integer"}}}),
        category: None,
    };
    let serialised   = serde_json::to_string(&original).unwrap();
    let deserialised: ToolDefinition = serde_json::from_str(&serialised).unwrap();
    assert_eq!(original.name, deserialised.name);
    assert_eq!(original.description, deserialised.description);
    assert_eq!(original.category, deserialised.category);
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRegistry — LLM output helpers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn openai_tools_array_is_array() {
    let r = registry_with_defaults();
    let v = r.openai_tools_array();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap().len(), 3);
}

#[test]
fn openai_tools_array_has_type_function() {
    let r    = registry_with_defaults();
    let arr  = r.openai_tools_array();
    let item = &arr[0];
    assert_eq!(item["type"], "function");
    assert!(item["function"]["name"].is_string());
    assert!(item["function"]["description"].is_string());
    assert!(item["function"]["parameters"].is_object());
}

#[test]
fn system_prompt_section_contains_tool_names() {
    let r = registry_with_defaults();
    let s = r.system_prompt_section();
    assert!(s.contains("echo"));
    assert!(s.contains("add"));
    assert!(s.contains("fail"));
}

#[test]
fn system_prompt_section_shows_category() {
    let r = registry_with_defaults();
    let s = r.system_prompt_section();
    assert!(s.contains("[math]"));
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolContext
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tool_context_default_is_empty() {
    let ctx = ToolContext::default();
    assert!(ctx.workspace_root.is_none());
    assert!(ctx.current_file.is_none());
    assert!(ctx.extras.is_empty());
}

#[test]
fn tool_context_builder_sets_workspace() {
    let ctx = ToolContext::new().with_workspace("/tmp/ws");
    assert_eq!(ctx.workspace_root.unwrap().to_str().unwrap(), "/tmp/ws");
}

#[test]
fn tool_context_builder_sets_current_file() {
    let ctx = ToolContext::new().with_current_file("/tmp/file.rs");
    assert_eq!(ctx.current_file.unwrap().to_str().unwrap(), "/tmp/file.rs");
}

#[test]
fn tool_context_extras_insert_and_retrieve() {
    let mut ctx = ToolContext::new();
    ctx.insert_extra("answer", 42u32);
    let retrieved = ctx.get_extra::<u32>("answer").copied();
    assert_eq!(retrieved, Some(42));
}

#[test]
fn tool_context_extras_wrong_type_returns_none() {
    let mut ctx = ToolContext::new();
    ctx.insert_extra("val", 42u32);
    let wrong = ctx.get_extra::<String>("val");
    assert!(wrong.is_none());
}

#[test]
fn tool_context_extras_missing_key_returns_none() {
    let ctx = ToolContext::new();
    assert!(ctx.get_extra::<u32>("nope").is_none());
}

#[test]
fn tool_context_clone_shares_extras_arc() {
    // insert_extra wraps the value in Arc, so we insert Arc<Mutex<u32>>
    // and get_extra::<Arc<Mutex<u32>>> to retrieve it.
    let inner = Arc::new(Mutex::new(0u32));
    let mut ctx = ToolContext::new();
    ctx.insert_extra("shared", Arc::clone(&inner));

    let ctx2 = ctx.clone();
    // Mutate through the original reference
    *inner.lock().unwrap() = 99;

    // Both contexts should see the change since they hold the same Arc
    let v1 = ctx.get_extra::<Arc<Mutex<u32>>>("shared")
        .map(|m| *m.lock().unwrap());
    let v2 = ctx2.get_extra::<Arc<Mutex<u32>>>("shared")
        .map(|m| *m.lock().unwrap());

    assert_eq!(v1, Some(99), "ctx should see updated value");
    assert_eq!(v2, Some(99), "ctx2 should share the same Arc");
}

// ─────────────────────────────────────────────────────────────────────────────
// tool_params! macro
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tool_params_empty() {
    let s = tool_params!();
    assert_eq!(s["type"], "object");
    assert!(s["properties"].is_object());
}

#[test]
fn tool_params_req_appears_in_required() {
    let s = tool_params! { req "name": string = "A name" };
    let required = s["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "name"));
}

#[test]
fn tool_params_opt_not_in_required() {
    let s = tool_params! { opt "limit": integer = "Max" };
    let required = s["required"].as_array().unwrap();
    assert!(!required.iter().any(|v| v == "limit"));
}

#[test]
fn tool_params_mixed_required_and_optional() {
    let s = tool_params! {
        req "query": string  = "Required param",
        opt "limit": integer = "Optional param",
    };
    let required = s["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "query"));
    assert!(!required.iter().any(|v| v == "limit"));
    assert_eq!(s["properties"]["query"]["type"], "string");
    assert_eq!(s["properties"]["limit"]["type"], "integer");
}

#[test]
fn tool_params_all_json_types() {
    let s = tool_params! {
        req "s": string  = "s",
        req "i": integer = "i",
        req "n": number  = "n",
        req "b": boolean = "b",
        req "o": object  = "o",
        req "a": array   = "a",
    };
    assert_eq!(s["properties"]["s"]["type"], "string");
    assert_eq!(s["properties"]["i"]["type"], "integer");
    assert_eq!(s["properties"]["n"]["type"], "number");
    assert_eq!(s["properties"]["b"]["type"], "boolean");
    assert_eq!(s["properties"]["o"]["type"], "object");
    assert_eq!(s["properties"]["a"]["type"], "array");
}

#[test]
fn tool_params_description_is_preserved() {
    let s = tool_params! { req "q": string = "my description" };
    assert_eq!(s["properties"]["q"]["description"], "my description");
}

// ─────────────────────────────────────────────────────────────────────────────
// PluginToolRegistry
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn plugin_tool_registry_new_is_empty() {
    let r = PluginToolRegistry::new();
    assert!(r.is_empty());
    assert_eq!(r.len(), 0);
}

#[test]
fn plugin_tool_registry_add_and_len() {
    let mut r = PluginToolRegistry::new();
    r.add(Arc::new(EchoTool));
    r.add(Arc::new(AddTool));
    assert_eq!(r.len(), 2);
    assert!(!r.is_empty());
}

#[test]
fn merge_plugin_into_registry() {
    let mut plugin_reg = PluginToolRegistry::new();
    plugin_reg.add(Arc::new(EchoTool));
    plugin_reg.add(Arc::new(AddTool));

    let mut registry = ToolRegistry::new();
    registry.merge_plugin(&plugin_reg);

    assert!(registry.contains("echo"));
    assert!(registry.contains("add"));
    assert_eq!(registry.names().len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolPlugin trait
// ─────────────────────────────────────────────────────────────────────────────

struct MathPlugin;
impl ToolPlugin for MathPlugin {
    fn name(&self) -> &'static str { "math_plugin" }
    fn tool_registry(&self) -> PluginToolRegistry {
        let mut r = PluginToolRegistry::new();
        r.add(Arc::new(AddTool));
        r
    }
}

struct TextPlugin;
impl ToolPlugin for TextPlugin {
    fn name(&self) -> &'static str { "text_plugin" }
    fn tool_registry(&self) -> PluginToolRegistry {
        let mut r = PluginToolRegistry::new();
        r.add(Arc::new(EchoTool));
        r
    }
}

#[test]
fn add_plugin_merges_tools() {
    let mut registry = ToolRegistry::new();
    registry.add_plugin(&MathPlugin);
    assert!(registry.contains("add"));
    assert!(!registry.contains("echo"));
}

#[test]
fn add_multiple_plugins() {
    let mut registry = ToolRegistry::new();
    registry.add_plugin(&MathPlugin);
    registry.add_plugin(&TextPlugin);
    assert!(registry.contains("add"));
    assert!(registry.contains("echo"));
    assert_eq!(registry.names().len(), 2);
}

#[test]
fn plugins_can_be_boxed_as_trait_objects() {
    let plugins: Vec<Box<dyn ToolPlugin>> = vec![
        Box::new(MathPlugin),
        Box::new(TextPlugin),
    ];
    let mut registry = ToolRegistry::new();
    for p in &plugins {
        registry.add_plugin(p.as_ref());
    }
    assert_eq!(registry.names().len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration: context flows into tool
// ─────────────────────────────────────────────────────────────────────────────

struct WorkspaceTool;
impl ChatTool for WorkspaceTool {
    fn name(&self)        -> &'static str { "workspace" }
    fn description(&self) -> &'static str { "Returns workspace root." }
    fn parameters_schema(&self) -> Value  { tool_params!() }
    fn execute(&self, _args: Value, ctx: &ToolContext) -> anyhow::Result<Value> {
        let root = ctx.workspace_root
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "none".to_string());
        Ok(json!({ "root": root }))
    }
}

#[test]
fn context_workspace_root_accessible_in_tool() {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(WorkspaceTool));

    let ctx = ToolContext::new().with_workspace("/my/project");
    let res = r.execute("workspace", json!({}), &ctx).unwrap();
    assert_eq!(res["root"], "/my/project");
}

struct ExtraTool;
impl ChatTool for ExtraTool {
    fn name(&self)        -> &'static str { "extra_tool" }
    fn description(&self) -> &'static str { "Reads a typed extra." }
    fn parameters_schema(&self) -> Value  { tool_params!() }
    fn execute(&self, _args: Value, ctx: &ToolContext) -> anyhow::Result<Value> {
        let val = ctx.get_extra::<u32>("magic_number").copied().unwrap_or(0);
        Ok(json!({ "value": val }))
    }
}

#[test]
fn context_extras_accessible_in_tool() {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(ExtraTool));

    let mut ctx = ToolContext::new();
    ctx.insert_extra("magic_number", 42u32);

    let res = r.execute("extra_tool", json!({}), &ctx).unwrap();
    assert_eq!(res["value"], 42);
}
