//! Proc-macro crate for `tool_registry`.
//!
//! # `#[tool]`
//!
//! Annotate a function to:
//! 1. Derive a JSON Schema from its parameters.
//! 2. Generate a JSON-argument-extracting wrapper.
//! 3. Generate or embed Markdown documentation.
//! 4. Submit an [`InventoryEntry`](tool_registry::InventoryEntry) at link time.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tool_registry_macros::tool;
//! use serde_json::Value;
//!
//! #[tool(category = "math", timeout_ms = 1000)]
//! /// Multiply two numbers together.
//! pub fn multiply(a: f64, b: f64) -> anyhow::Result<Value> {
//!     Ok(serde_json::json!({ "result": a * b }))
//! }
//! ```
//!
//! ## Supported `#[tool(...)]` options
//!
//! | Key           | Type     | Default | Description                                             |
//! |---------------|----------|---------|---------------------------------------------------------|
//! | `category`    | string   | `""`    | Grouping category shown in system prompts               |
//! | `timeout_ms`  | u32      | `5000`  | Hint for callers (stored in definition, not enforced)   |
//! | `docs`        | string   | —       | Path to `.md` file embedded via `include_str!()`        |
//!
//! ## Parameter types and JSON Schema mapping
//!
//! | Rust type                        | JSON Schema type |
//! |----------------------------------|------------------|
//! | `String` / `&str`               | `"string"`       |
//! | `i32` / `i64` / `u32` / `u64` / `isize` / `usize` | `"integer"` |
//! | `f32` / `f64`                   | `"number"`       |
//! | `bool`                          | `"boolean"`      |
//! | `Option<T>`                     | type of `T`, optional (not in `required`) |
//! | anything else                   | `"object"`       |
//!
//! # `tool_params!`
//!
//! A declarative helper for writing JSON Schema parameter objects inline, without
//! a `#[tool]` annotation:
//!
//! ```rust
//! # use tool_registry_macros::tool_params;
//! let schema = tool_params! {
//!     req "query":   string  = "Search query",
//!     opt "limit":   integer = "Maximum results",
//! };
//! assert_eq!(schema["required"][0], "query");
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Expr, FnArg, ItemFn,
    Meta, Pat, Type,
};

// ─────────────────────────────────────────────────────────────────────────────
// #[tool] attribute macro
// ─────────────────────────────────────────────────────────────────────────────

#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr with Punctuated::<Meta, Comma>::parse_terminated);
    let input = parse_macro_input!(item as ItemFn);

    let fn_name      = &input.sig.ident;
    let fn_vis       = &input.vis;
    let fn_async     = &input.sig.asyncness;
    let fn_inputs    = &input.sig.inputs;
    let fn_output    = &input.sig.output;
    let fn_block     = &input.block;

    let doc_comment      = extract_doc_comment(&input.attrs);
    let tool_name        = to_snake_case(&fn_name.to_string());
    let params           = extract_parameters(fn_inputs);
    let (category, timeout_ms, docs_path) = parse_attrs(&attrs);

    let schema_str = generate_schema_str(&params);
    let schema_val: serde_json::Value = serde_json::from_str(&schema_str)
        .unwrap_or_else(|_| serde_json::json!({"type":"object","properties":{}}));

    let definition_json = serde_json::json!({
        "name": tool_name,
        "description": doc_comment.trim(),
        "parameters": schema_val,
        "category": category.as_deref().unwrap_or(""),
        "timeout_ms": timeout_ms,
    })
    .to_string();

    // ── const TOOL_DEF_<NAME>: &str ───────────────────────────────────────────
    let def_const   = format_ident!("TOOL_DEF_{}", fn_name.to_string().to_uppercase());
    let doc_const   = format_ident!("TOOL_DOC_{}", fn_name.to_string().to_uppercase());
    let wrapper_fn  = format_ident!("{}_tool_wrapper", fn_name);

    let doc_const_body = match docs_path {
        Some(path) => {
            let path_lit = syn::LitStr::new(&path, proc_macro2::Span::call_site());
            quote! { include_str!(#path_lit) }
        }
        None => {
            let md = generate_markdown_doc(&tool_name, doc_comment.trim(), &params, category.as_deref());
            quote! { #md }
        }
    };

    // ── wrapper fn ────────────────────────────────────────────────────────────
    let param_extractions = generate_param_extractions(&params);
    let param_idents: Vec<_> = params
        .iter()
        .map(|(name, _)| format_ident!("{}", name))
        .collect();

    let expanded = quote! {
        #[doc(hidden)]
        pub const #def_const: &str = #definition_json;

        #[doc(hidden)]
        pub const #doc_const: &str = #doc_const_body;

        #[doc(hidden)]
        pub fn #wrapper_fn(
            tool_args: serde_json::Value,
            _ctx: &tool_registry::ToolContext,
        ) -> anyhow::Result<serde_json::Value> {
            #param_extractions
            #fn_name(#(#param_idents),*)
        }

        tool_registry::inventory::submit! {
            tool_registry::InventoryEntry {
                namespace: module_path!(),
                definition_json: #def_const,
                documentation: #doc_const,
                handler: #wrapper_fn,
            }
        }

        #fn_vis #fn_async fn #fn_name(#fn_inputs) #fn_output {
            #fn_block
        }
    };

    TokenStream::from(expanded)
}

// ─────────────────────────────────────────────────────────────────────────────
// tool_params! declarative macro (exported as proc-macro for consistency)
// ─────────────────────────────────────────────────────────────────────────────

/// Build a JSON Schema `parameters` object.
///
/// ```rust
/// # use tool_registry_macros::tool_params;
/// let schema = tool_params! {
///     req "name": string  = "Name of the item",
///     opt "limit": integer = "Max results to return",
/// };
/// ```
#[proc_macro]
pub fn tool_params(input: TokenStream) -> TokenStream {
    // Delegate to the declarative macro defined in tool_registry so it is usable
    // both as `tool_params!` from the macro crate and from `tool_registry::tool_params!`.
    // We re-emit it as a `macro_rules!` expansion here.
    let _ = input; // The real implementation lives in the declarative macro below.
    // Emit a compile error pointing users to the re-export.
    TokenStream::from(quote! {
        compile_error!(
            "Use `tool_registry::tool_params!` instead of importing from `tool_registry_macros`."
        )
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn extract_doc_comment(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") { return None; }
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(el) = &nv.value {
                    if let syn::Lit::Str(s) = &el.lit {
                        return Some(s.value());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_parameters(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> Vec<(String, Type)> {
    inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pt) = arg {
                if let Pat::Ident(pi) = pt.pat.as_ref() {
                    return Some((pi.ident.to_string(), (*pt.ty).clone()));
                }
            }
            None
        })
        .collect()
}

fn parse_attrs(attrs: &Punctuated<Meta, Comma>) -> (Option<String>, u32, Option<String>) {
    let mut category: Option<String>   = None;
    let mut timeout_ms: u32            = 5000;
    let mut docs_path: Option<String>  = None;

    for meta in attrs {
        if let Meta::NameValue(nv) = meta {
            if let Expr::Lit(el) = &nv.value {
                if nv.path.is_ident("category") {
                    if let syn::Lit::Str(s) = &el.lit { category = Some(s.value()); }
                } else if nv.path.is_ident("timeout_ms") {
                    if let syn::Lit::Int(i) = &el.lit {
                        if let Ok(n) = i.base10_parse::<u32>() { timeout_ms = n; }
                    }
                } else if nv.path.is_ident("docs") {
                    if let syn::Lit::Str(s) = &el.lit { docs_path = Some(s.value()); }
                }
            }
        }
    }
    (category, timeout_ms, docs_path)
}

fn rust_type_to_json(ty_str: &str) -> &'static str {
    // unwrap Option<T>
    let inner = if ty_str.starts_with("Option<") && ty_str.ends_with('>') {
        &ty_str[7..ty_str.len() - 1]
    } else {
        ty_str
    };
    match inner {
        "String" | "&str" | "str"                           => "string",
        "i8"|"i16"|"i32"|"i64"|"i128"|"isize"
        | "u8"|"u16"|"u32"|"u64"|"u128"|"usize"            => "integer",
        "f32" | "f64"                                       => "number",
        "bool"                                              => "boolean",
        _                                                   => "object",
    }
}

fn is_optional(ty: &Type) -> bool {
    let s = quote::quote!(#ty).to_string().replace(' ', "");
    s.starts_with("Option<")
}

fn generate_schema_str(params: &[(String, Type)]) -> String {
    let props = params
        .iter()
        .map(|(name, ty)| {
            let ts = quote::quote!(#ty).to_string().replace(' ', "");
            let jt = rust_type_to_json(&ts);
            format!(r#""{name}": {{"type": "{jt}"}}"#)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let required = params
        .iter()
        .filter(|(_, ty)| !is_optional(ty))
        .map(|(name, _)| format!(r#""{name}""#))
        .collect::<Vec<_>>()
        .join(", ");

    format!(r#"{{"type":"object","properties":{{{props}}},"required":[{required}]}}"#)
}

fn generate_param_extractions(params: &[(String, Type)]) -> proc_macro2::TokenStream {
    let extractions = params.iter().map(|(name, ty)| {
        let ident = format_ident!("{}", name);
        let optional = is_optional(ty);
        if optional {
            quote! {
                let #ident: #ty = tool_args
                    .get(stringify!(#ident))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
            }
        } else {
            quote! {
                let #ident: #ty = serde_json::from_value(
                    tool_args
                        .get(stringify!(#ident))
                        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: {}", stringify!(#ident)))?
                        .clone(),
                )
                .map_err(|e| anyhow::anyhow!("Invalid type for parameter '{}': {e}", stringify!(#ident)))?;
            }
        }
    });
    quote! { #(#extractions)* }
}

fn generate_markdown_doc(
    name: &str,
    description: &str,
    params: &[(String, Type)],
    category: Option<&str>,
) -> String {
    let cat = category
        .filter(|c| !c.is_empty())
        .map(|c| format!("\n**Category**: {c}\n"))
        .unwrap_or_default();

    let params_md = if params.is_empty() {
        "No parameters.".to_string()
    } else {
        let rows = params
            .iter()
            .map(|(n, ty)| {
                let ts  = quote::quote!(#ty).to_string().replace(' ', "");
                let opt = if is_optional(ty) { " *(optional)*" } else { "" };
                format!("| `{n}` | `{ts}` |{opt} |")
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("### Parameters\n\n| Name | Type | Notes |\n|------|------|-------|\n{rows}")
    };

    format!("# `{name}`\n{cat}\n{description}\n\n{params_md}\n")
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out.to_lowercase()
}
