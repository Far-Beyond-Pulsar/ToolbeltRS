//! Tests for `tool_registry_builtin` — built-in web tools.

use std::sync::Arc;
use serde_json::json;
use tool_registry::{ChatTool, ToolContext, ToolRegistry};
use tool_registry_builtin::{FetchUrlTool, WebSearchTool, register_builtins};

// ─────────────────────────────────────────────────────────────────────────────
// Registration / metadata
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn web_search_tool_has_correct_name() {
    let t = WebSearchTool;
    assert_eq!(t.name(), "web_search");
}

#[test]
fn fetch_url_tool_has_correct_name() {
    let t = FetchUrlTool;
    assert_eq!(t.name(), "fetch_url");
}

#[test]
fn web_search_tool_has_description() {
    let t = WebSearchTool;
    assert!(!t.description().is_empty());
}

#[test]
fn fetch_url_tool_has_description() {
    let t = FetchUrlTool;
    assert!(!t.description().is_empty());
}

#[test]
fn web_search_parameters_schema_is_object() {
    let t   = WebSearchTool;
    let s   = t.parameters_schema();
    assert_eq!(s["type"], "object");
    assert!(s["properties"]["query"].is_object());
}

#[test]
fn fetch_url_parameters_schema_has_url_param() {
    let t = FetchUrlTool;
    let s = t.parameters_schema();
    assert_eq!(s["type"], "object");
    assert!(s["properties"]["url"].is_object());
}

#[test]
fn register_builtins_adds_both_tools() {
    let mut r = ToolRegistry::new();
    register_builtins(&mut r);
    assert!(r.contains("web_search"));
    assert!(r.contains("fetch_url"));
}

#[test]
fn register_builtins_exactly_two_tools() {
    let mut r = ToolRegistry::new();
    register_builtins(&mut r);
    assert_eq!(r.names().len(), 2);
}

#[test]
fn builtin_tools_have_definitions() {
    let mut r = ToolRegistry::new();
    register_builtins(&mut r);
    let defs = r.definitions();
    assert_eq!(defs.len(), 2);
    assert!(defs.iter().any(|d| d.name == "web_search"));
    assert!(defs.iter().any(|d| d.name == "fetch_url"));
}

#[test]
fn individual_registration_works() {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(WebSearchTool));
    r.register(Arc::new(FetchUrlTool));
    assert!(r.contains("web_search"));
    assert!(r.contains("fetch_url"));
}

#[test]
fn builtin_tools_appear_in_openai_array() {
    let mut r = ToolRegistry::new();
    register_builtins(&mut r);
    let arr = r.openai_tools_array();
    let a   = arr.as_array().unwrap();
    assert_eq!(a.len(), 2);
    assert!(a.iter().all(|v| v["type"] == "function"));
}

// ─────────────────────────────────────────────────────────────────────────────
// fetch_url — error handling without network
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fetch_url_rejects_non_http_scheme() {
    let tool = FetchUrlTool;
    let ctx  = ToolContext::default();
    let res  = tool.execute(json!({ "url": "ftp://example.com/file" }), &ctx).unwrap();
    // Should return ok:false, not panic or return Err
    assert_eq!(res["ok"], false);
}

#[test]
fn fetch_url_rejects_empty_url() {
    let tool = FetchUrlTool;
    let ctx  = ToolContext::default();
    let res  = tool.execute(json!({ "url": "" }), &ctx).unwrap();
    assert_eq!(res["ok"], false);
}

#[test]
fn fetch_url_missing_url_param_returns_err() {
    let tool = FetchUrlTool;
    let ctx  = ToolContext::default();
    // Missing required "url" field — should return Err or ok:false
    let result = tool.execute(json!({}), &ctx);
    match result {
        Err(_) => {} // acceptable
        Ok(v)  => assert_eq!(v["ok"], false, "expected error or ok:false"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// web_search — error handling without network
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn web_search_missing_query_returns_err_or_false() {
    let tool   = WebSearchTool;
    let ctx    = ToolContext::default();
    let result = tool.execute(json!({}), &ctx);
    match result {
        Err(_) => {} // acceptable
        Ok(v)  => assert_eq!(v["ok"], false, "expected error or ok:false"),
    }
}
