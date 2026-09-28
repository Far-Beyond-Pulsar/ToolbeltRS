//! Integration tests for `tool_registry_macros` — the `#[tool]` proc-macro.
//!
//! Each test lives in this crate so the linker collects the inventory entries
//! emitted by the macro before `main` (or the test harness) runs.

use serde_json::{json, Value};
use tool_registry::{InventoryEntry, PluginToolRegistry};
use tool_registry_macros::tool;

// ─────────────────────────────────────────────────────────────────────────────
// Tools under test (each annotated with #[tool] at module scope)
// ─────────────────────────────────────────────────────────────────────────────

/// Greets a person by name.
#[tool]
pub fn greet(name: String) -> anyhow::Result<Value> {
    Ok(json!({ "greeting": format!("hello, {name}!") }))
}

/// Adds two integers.
#[tool(category = "math")]
pub fn add_ints(a: i64, b: i64) -> anyhow::Result<Value> {
    Ok(json!({ "result": a + b }))
}

/// Returns a default greeting when no name is provided.
#[tool]
pub fn maybe_greet(name: Option<String>) -> anyhow::Result<Value> {
    let who = name.unwrap_or_else(|| "world".to_string());
    Ok(json!({ "greeting": format!("hi, {who}") }))
}

/// Tool with all basic types.
#[tool]
pub fn all_types(
    s: String,
    i: i64,
    f: f64,
    b: bool,
    opt_s: Option<String>,
) -> anyhow::Result<Value> {
    Ok(json!({ "s": s, "i": i, "f": f, "b": b, "opt_s": opt_s }))
}

/// Always fails.
#[tool]
pub fn always_fail() -> anyhow::Result<Value> {
    anyhow::bail!("deliberate failure")
}

/// Place points in the world.
///
/// # Arguments
/// * `points` - World positions as `[x, y, z]`.
/// * `tags` - Free-form labels.
/// * `extra` - Any JSON payload.
#[tool]
pub fn place_points(
    points: Vec<[f32; 3]>,
    tags: Option<Vec<String>>,
    extra: Option<Value>,
) -> anyhow::Result<Value> {
    Ok(json!({ "count": points.len(), "tags": tags, "extra": extra }))
}

/// Reports the file from the caller's context.
#[tool]
pub fn current_file(ctx: &tool_registry::ToolContext, suffix: String) -> anyhow::Result<Value> {
    let file = ctx.current_file.as_ref().map(|p| p.display().to_string());
    Ok(json!({ "file": file, "suffix": suffix }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn ctx() -> tool_registry::ToolContext {
    tool_registry::ToolContext::default()
}

fn entry_for_tool(fn_name: &str) -> Option<&'static InventoryEntry> {
    inventory::iter::<InventoryEntry>
        .into_iter()
        .find(|e| {
            let def: Value = serde_json::from_str(e.definition_json).unwrap_or_default();
            def["name"] == fn_name
        })
}

fn def(fn_name: &str) -> Value {
    let e = entry_for_tool(fn_name)
        .unwrap_or_else(|| panic!("no inventory entry for `{fn_name}`"));
    serde_json::from_str(e.definition_json).unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// Inventory presence
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tool_macro_registers_greet_in_inventory() {
    assert!(entry_for_tool("greet").is_some(), "greet not in inventory");
}

#[test]
fn tool_macro_registers_add_ints_in_inventory() {
    assert!(entry_for_tool("add_ints").is_some(), "add_ints not in inventory");
}

#[test]
fn tool_macro_registers_maybe_greet_in_inventory() {
    assert!(entry_for_tool("maybe_greet").is_some(), "maybe_greet not in inventory");
}

#[test]
fn tool_macro_registers_all_types_in_inventory() {
    assert!(entry_for_tool("all_types").is_some(), "all_types not in inventory");
}

// ─────────────────────────────────────────────────────────────────────────────
// Definition JSON shape
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn definition_has_correct_name() {
    let d = def("greet");
    assert_eq!(d["name"], "greet");
}

#[test]
fn definition_description_from_doc_comment() {
    let d    = def("greet");
    let desc = d["description"].as_str().unwrap();
    assert!(desc.contains("Greets"), "expected doc comment in description, got: {desc}");
}

#[test]
fn definition_has_parameters_object() {
    let d = def("greet");
    assert_eq!(d["parameters"]["type"], "object");
    assert!(d["parameters"]["properties"].is_object());
}

#[test]
fn required_param_in_required_array() {
    let d        = def("greet");
    let required = d["parameters"]["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "name"), "name should be required");
}

#[test]
fn option_param_not_in_required_array() {
    let d        = def("maybe_greet");
    let required = d["parameters"]["required"].as_array().unwrap();
    assert!(!required.iter().any(|v| v == "name"), "Option<> param should not be required");
}

#[test]
fn category_stored_in_definition() {
    let d = def("add_ints");
    assert_eq!(d["category"], "math");
}

#[test]
fn category_empty_string_when_not_specified() {
    // The macro stores category as "" when not provided
    let d   = def("greet");
    let cat = d["category"].as_str();
    assert!(
        cat.is_none() || cat == Some(""),
        "category should be empty or absent, got: {:?}", cat
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Type mapping in schema
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn string_param_maps_to_schema_string() {
    let d = def("all_types");
    assert_eq!(d["parameters"]["properties"]["s"]["type"], "string");
}

#[test]
fn integer_param_maps_to_schema_integer() {
    let d = def("all_types");
    assert_eq!(d["parameters"]["properties"]["i"]["type"], "integer");
}

#[test]
fn float_param_maps_to_schema_number() {
    let d = def("all_types");
    assert_eq!(d["parameters"]["properties"]["f"]["type"], "number");
}

#[test]
fn bool_param_maps_to_schema_boolean() {
    let d = def("all_types");
    assert_eq!(d["parameters"]["properties"]["b"]["type"], "boolean");
}

#[test]
fn option_string_maps_to_string_type() {
    // inner type of Option<String> is "string"
    let d = def("all_types");
    assert_eq!(d["parameters"]["properties"]["opt_s"]["type"], "string");
}

// ─────────────────────────────────────────────────────────────────────────────
// Wrapper function dispatch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wrapper_dispatches_to_original_function() {
    let result = greet_tool_wrapper(json!({ "name": "rustacean" }), &ctx()).unwrap();
    assert_eq!(result["greeting"], "hello, rustacean!");
}

#[test]
fn wrapper_passes_integer_args() {
    let result = add_ints_tool_wrapper(json!({ "a": 10, "b": 32 }), &ctx()).unwrap();
    assert_eq!(result["result"], 42);
}

#[test]
fn wrapper_optional_param_present() {
    let result = maybe_greet_tool_wrapper(json!({ "name": "Alice" }), &ctx()).unwrap();
    assert_eq!(result["greeting"], "hi, Alice");
}

#[test]
fn wrapper_optional_param_absent_uses_default() {
    let result = maybe_greet_tool_wrapper(json!({}), &ctx()).unwrap();
    assert_eq!(result["greeting"], "hi, world");
}

#[test]
fn wrapper_propagates_tool_error() {
    let err = always_fail_tool_wrapper(json!({}), &ctx());
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("deliberate failure"));
}

#[test]
fn wrapper_returns_err_for_missing_required_param() {
    // greet requires "name" — omit it
    let err = greet_tool_wrapper(json!({}), &ctx());
    assert!(err.is_err(), "expected error for missing required param");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("name") || msg.contains("Missing") || msg.contains("required"),
        "unexpected error message: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// PluginToolRegistry::from_namespace with #[tool] tools
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn from_namespace_collects_tools_from_this_module() {
    let ns  = module_path!();
    let reg = PluginToolRegistry::from_namespace(ns);
    assert!(!reg.is_empty(), "expected tools to be collected for namespace `{ns}`");
}

#[test]
fn from_namespace_excludes_other_namespaces() {
    let reg = PluginToolRegistry::from_namespace("_nonexistent_prefix_xyz_");
    assert!(reg.is_empty());
}

#[test]
fn from_namespace_greet_is_executable() {
    let ns   = module_path!();
    let preg = PluginToolRegistry::from_namespace(ns);

    let mut registry = tool_registry::ToolRegistry::new();
    registry.merge_plugin(&preg);

    let ctx    = tool_registry::ToolContext::default();
    let result = registry.execute("greet", json!({ "name": "test" }), &ctx).unwrap();
    assert_eq!(result["greeting"], "hello, test!");
}

#[test]
fn from_namespace_all_tools_present() {
    let ns   = module_path!();
    let preg = PluginToolRegistry::from_namespace(ns);

    let mut registry = tool_registry::ToolRegistry::new();
    registry.merge_plugin(&preg);

    assert!(registry.contains("greet"));
    assert!(registry.contains("add_ints"));
    assert!(registry.contains("maybe_greet"));
    assert!(registry.contains("all_types"));
    assert!(registry.contains("always_fail"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Containers, argument docs and context injection
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn vec_of_fixed_arrays_maps_to_nested_array_schema() {
    let d = def("place_points");
    let points = &d["parameters"]["properties"]["points"];
    assert_eq!(points["type"], "array");
    assert_eq!(points["items"]["type"], "array");
    assert_eq!(points["items"]["items"]["type"], "number");
    assert_eq!(points["items"]["minItems"], 3);
    assert_eq!(points["items"]["maxItems"], 3);
    assert_eq!(d["parameters"]["properties"]["tags"]["items"]["type"], "string");
}

#[test]
fn value_param_has_open_schema() {
    let d = def("place_points");
    assert!(d["parameters"]["properties"]["extra"].get("type").is_none());
}

#[test]
fn arguments_section_becomes_param_descriptions() {
    let d = def("place_points");
    assert_eq!(d["description"], "Place points in the world.");
    assert_eq!(
        d["parameters"]["properties"]["points"]["description"],
        "World positions as `[x, y, z]`."
    );
}

#[test]
fn wrapper_rejects_malformed_optional_param() {
    let err = place_points_tool_wrapper(json!({ "points": [], "tags": 5 }), &ctx());
    assert!(err.unwrap_err().to_string().contains("tags"));
}

#[test]
fn wrapper_treats_null_optional_as_absent() {
    let result =
        place_points_tool_wrapper(json!({ "points": [[1, 2, 3]], "tags": null }), &ctx()).unwrap();
    assert_eq!(result["count"], 1);
    assert!(result["tags"].is_null());
}

#[test]
fn context_param_is_injected_and_hidden_from_schema() {
    let d = def("current_file");
    let props = d["parameters"]["properties"].as_object().unwrap();
    assert_eq!(props.len(), 1, "only `suffix` should be in the schema: {props:?}");
    assert_eq!(d["parameters"]["required"], json!(["suffix"]));

    let ctx = tool_registry::ToolContext::new().with_current_file("level.level");
    let result = current_file_tool_wrapper(json!({ "suffix": "x" }), &ctx).unwrap();
    assert_eq!(result["file"], "level.level");
}
