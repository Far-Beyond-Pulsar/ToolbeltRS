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
//! ///
//! /// # Arguments
//! /// * `a` - Left-hand factor.
//! /// * `b` - Right-hand factor.
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
//! ## Parameter descriptions
//!
//! A rustdoc `# Arguments` (or `# Parameters`) section in the doc comment is
//! lifted out of the tool description and attached to each parameter's schema
//! as its `description`. Entries are list items naming the parameter in
//! backticks: ``* `name` - what it is``. Indented lines continue the entry.
//!
//! ## Tool context
//!
//! A parameter of type `&ToolContext` (any path ending in `ToolContext`) is not
//! part of the schema: the wrapper passes the caller's context through to it.
//!
//! ## Parameter types and JSON Schema mapping
//!
//! | Rust type                        | JSON Schema type |
//! |----------------------------------|------------------|
//! | `String` / `&str` / `PathBuf`    | `"string"`       |
//! | `i32` / `i64` / `u32` / `u64` / `isize` / `usize` | `"integer"` |
//! | `f32` / `f64`                   | `"number"`       |
//! | `bool`                          | `"boolean"`      |
//! | `Vec<T>` / `HashSet<T>` / `[T]` | `"array"` of `T` |
//! | `[T; N]`                        | `"array"` of `T`, exactly `N` items |
//! | `serde_json::Value`             | any JSON value (no `type`) |
//! | `Option<T>`                     | schema of `T`, optional (not in `required`) |
//! | anything else                   | `"object"`       |
//!
//! # `tool_params!`
//!
//! A declarative helper for writing JSON Schema parameter objects inline, without
//! a `#[tool]` annotation:
//!
//! ```rust
//! # use tool_registry::tool_params;
//! let schema = tool_params! {
//!     req "query":   string  = "Search query",
//!     opt "limit":   integer = "Maximum results",
//! };
//! assert_eq!(schema["required"][0], "query");
//! ```

use std::collections::BTreeMap;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use serde_json::{json, Value};
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Expr, FnArg, GenericArgument,
    ItemFn, Meta, Pat, PathArguments, Type,
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

    let (description, arg_docs) = parse_doc_comment(&doc_lines(&input.attrs));
    let tool_name        = to_snake_case(&fn_name.to_string());
    let params           = extract_parameters(fn_inputs);
    let (category, timeout_ms, docs_path) = parse_attrs(&attrs);

    let schema_val = generate_schema(&params, &arg_docs);

    let definition_json = json!({
        "name": tool_name,
        "description": description,
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
            let md = generate_markdown_doc(&tool_name, &description, &params, &arg_docs, category.as_deref());
            quote! { #md }
        }
    };

    // ── wrapper fn ────────────────────────────────────────────────────────────
    let param_extractions = generate_param_extractions(&params);
    let call_args: Vec<_> = params
        .iter()
        .map(|param| match param {
            Param::Arg { name, .. } => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            Param::Context => quote! { __tool_ctx },
        })
        .collect();

    let expanded = quote! {
        #[doc(hidden)]
        pub const #def_const: &str = #definition_json;

        #[doc(hidden)]
        pub const #doc_const: &str = #doc_const_body;

        #[doc(hidden)]
        pub fn #wrapper_fn(
            tool_args: serde_json::Value,
            __tool_ctx: &tool_registry::ToolContext,
        ) -> anyhow::Result<serde_json::Value> {
            let _ = __tool_ctx;
            tool_registry::tracing::debug!(tool = #tool_name, "tool macro wrapper start");
            #param_extractions
            let result = #fn_name(#(#call_args),*);
            tool_registry::tracing::debug!(tool = #tool_name, success = result.is_ok(), "tool macro wrapper end");
            result
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
/// # use tool_registry::tool_params;
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
// Parameters
// ─────────────────────────────────────────────────────────────────────────────

/// One function parameter as the macro sees it.
enum Param {
    /// Extracted from the JSON arguments and described in the schema.
    Arg { name: String, ty: Type },
    /// `&ToolContext`: handed the caller's context, invisible to the LLM.
    Context,
}

fn extract_parameters(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> Vec<Param> {
    inputs
        .iter()
        .filter_map(|arg| {
            let FnArg::Typed(pt) = arg else { return None };
            if is_tool_context(&pt.ty) {
                return Some(Param::Context);
            }
            let Pat::Ident(pi) = pt.pat.as_ref() else { return None };
            Some(Param::Arg { name: pi.ident.to_string(), ty: (*pt.ty).clone() })
        })
        .collect()
}

fn schema_args(params: &[Param]) -> impl Iterator<Item = (&str, &Type)> {
    params.iter().filter_map(|param| match param {
        Param::Arg { name, ty } => Some((name.as_str(), ty)),
        Param::Context => None,
    })
}

fn is_tool_context(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else { return false };
    let Type::Path(path) = reference.elem.as_ref() else { return false };
    path.path.segments.last().is_some_and(|s| s.ident == "ToolContext")
}

// ─────────────────────────────────────────────────────────────────────────────
// Doc comments
// ─────────────────────────────────────────────────────────────────────────────

fn doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
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
        .flat_map(|doc| doc.lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

/// Split a doc comment into the tool description and per-argument docs taken
/// from its `# Arguments` / `# Parameters` section.
///
/// The description keeps paragraph breaks; lines inside a paragraph are joined
/// with spaces, since LLM tool descriptions are free text, not rustdoc.
fn parse_doc_comment(lines: &[String]) -> (String, BTreeMap<String, String>) {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut arg_docs: BTreeMap<String, String> = BTreeMap::new();
    let mut last_arg: Option<String> = None;
    let mut in_args = false;

    fn flush(current: &mut String, paragraphs: &mut Vec<String>) {
        if !current.is_empty() {
            paragraphs.push(std::mem::take(current));
        }
    }

    for raw in lines {
        let line = raw.trim();
        if let Some(heading) = line.strip_prefix('#') {
            let heading = heading.trim_start_matches('#').trim().to_ascii_lowercase();
            in_args = matches!(heading.as_str(), "arguments" | "parameters" | "args" | "params");
            last_arg = None;
            if in_args {
                flush(&mut current, &mut paragraphs);
                continue;
            }
        }

        if in_args {
            if let Some((name, doc)) = parse_arg_line(line) {
                arg_docs.insert(name.clone(), doc);
                last_arg = Some(name);
            } else if !line.is_empty() {
                if let Some(doc) = last_arg.as_ref().and_then(|name| arg_docs.get_mut(name)) {
                    if !doc.is_empty() { doc.push(' '); }
                    doc.push_str(line);
                }
            }
            continue;
        }

        if line.is_empty() {
            flush(&mut current, &mut paragraphs);
        } else {
            if !current.is_empty() { current.push(' '); }
            current.push_str(line);
        }
    }
    flush(&mut current, &mut paragraphs);

    (paragraphs.join("\n\n"), arg_docs)
}

/// Parse ``* `name` - description`` (also `-` bullets, `:` separators and
/// unquoted names).
fn parse_arg_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('*').or_else(|| line.strip_prefix('-'))?.trim_start();
    let (name, rest) = if let Some(quoted) = rest.strip_prefix('`') {
        let end = quoted.find('`')?;
        (&quoted[..end], &quoted[end + 1..])
    } else {
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        (&rest[..end], &rest[end..])
    };
    if name.is_empty() {
        return None;
    }
    let doc = rest
        .trim_start()
        .trim_start_matches(|c| c == '-' || c == ':' || c == '—' || c == '–')
        .trim();
    Some((name.to_string(), doc.to_string()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Attributes
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// JSON Schema
// ─────────────────────────────────────────────────────────────────────────────

/// First generic type argument of a path segment (`T` in `Vec<T>`).
fn first_type_arg(segment: &syn::PathSegment) -> Option<&Type> {
    let PathArguments::AngleBracketed(args) = &segment.arguments else { return None };
    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

/// The JSON Schema describing values of `ty`.
fn type_schema(ty: &Type) -> Value {
    match ty {
        Type::Reference(reference) => type_schema(&reference.elem),
        Type::Paren(paren) => type_schema(&paren.elem),
        Type::Group(group) => type_schema(&group.elem),
        Type::Slice(slice) => json!({ "type": "array", "items": type_schema(&slice.elem) }),
        Type::Array(array) => {
            let mut schema = json!({ "type": "array", "items": type_schema(&array.elem) });
            if let Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(len), .. }) = &array.len {
                if let Ok(len) = len.base10_parse::<u64>() {
                    schema["minItems"] = json!(len);
                    schema["maxItems"] = json!(len);
                }
            }
            schema
        }
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return json!({ "type": "object" });
            };
            match segment.ident.to_string().as_str() {
                "Option" | "Box" | "Arc" | "Rc" => {
                    first_type_arg(segment).map(type_schema).unwrap_or_else(|| json!({}))
                }
                "Vec" | "VecDeque" | "HashSet" | "BTreeSet" => json!({
                    "type": "array",
                    "items": first_type_arg(segment).map(type_schema).unwrap_or_else(|| json!({})),
                }),
                "String" | "str" | "char" | "PathBuf" | "Path" => json!({ "type": "string" }),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => json!({ "type": "integer" }),
                "f32" | "f64" => json!({ "type": "number" }),
                "bool" => json!({ "type": "boolean" }),
                // Any JSON value: leave the type open.
                "Value" => json!({}),
                _ => json!({ "type": "object" }),
            }
        }
        _ => json!({ "type": "object" }),
    }
}

fn is_optional(ty: &Type) -> bool {
    let s = quote::quote!(#ty).to_string().replace(' ', "");
    s.starts_with("Option<")
}

fn generate_schema(params: &[Param], arg_docs: &BTreeMap<String, String>) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, ty) in schema_args(params) {
        let mut schema = type_schema(ty);
        if let Some(doc) = arg_docs.get(name).filter(|doc| !doc.is_empty()) {
            schema["description"] = json!(doc);
        }
        properties.insert(name.to_string(), schema);
        if !is_optional(ty) {
            required.push(json!(name));
        }
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

// ─────────────────────────────────────────────────────────────────────────────
// Wrapper
// ─────────────────────────────────────────────────────────────────────────────

fn generate_param_extractions(params: &[Param]) -> proc_macro2::TokenStream {
    let extractions = schema_args(params).map(|(name, ty)| {
        let ident = format_ident!("{}", name);
        if is_optional(ty) {
            // Absent or null means `None`; a present value of the wrong shape
            // is an error rather than a silent `None`, so a malformed argument
            // is reported back to the model instead of being ignored.
            quote! {
                let #ident: #ty = match tool_args.get(stringify!(#ident)) {
                    None | Some(serde_json::Value::Null) => None,
                    Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
                        anyhow::anyhow!("Invalid type for parameter '{}': {e}", stringify!(#ident))
                    })?,
                };
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

// ─────────────────────────────────────────────────────────────────────────────
// Markdown documentation
// ─────────────────────────────────────────────────────────────────────────────

fn generate_markdown_doc(
    name: &str,
    description: &str,
    params: &[Param],
    arg_docs: &BTreeMap<String, String>,
    category: Option<&str>,
) -> String {
    let cat = category
        .filter(|c| !c.is_empty())
        .map(|c| format!("\n**Category**: {c}\n"))
        .unwrap_or_default();

    let rows = schema_args(params)
        .map(|(n, ty)| {
            let ts  = quote::quote!(#ty).to_string().replace(' ', "");
            let opt = if is_optional(ty) { " *(optional)*" } else { "" };
            let doc = arg_docs.get(n).map(|d| format!(" {d}")).unwrap_or_default();
            format!("| `{n}` | `{ts}` |{doc}{opt} |")
        })
        .collect::<Vec<_>>();

    let params_md = if rows.is_empty() {
        "No parameters.".to_string()
    } else {
        format!(
            "### Parameters\n\n| Name | Type | Notes |\n|------|------|-------|\n{}",
            rows.join("\n")
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(doc: &str) -> Vec<String> {
        doc.lines().map(str::to_string).collect()
    }

    #[test]
    fn arguments_section_is_lifted_out_of_the_description() {
        let (description, args) = parse_doc_comment(&lines(
            " Move an object.\n\n Second paragraph\n continues.\n\n # Arguments\n * `id` - Object id.\n * `position` - World position,\n   in metres.\n",
        ));
        assert_eq!(description, "Move an object.\n\nSecond paragraph continues.");
        assert_eq!(args["id"], "Object id.");
        assert_eq!(args["position"], "World position, in metres.");
    }

    #[test]
    fn later_headings_end_the_arguments_section() {
        let (description, args) =
            parse_doc_comment(&lines(" Do it.\n # Parameters\n - name: Who.\n # Returns\n Stuff."));
        assert_eq!(args["name"], "Who.");
        assert!(description.contains("Returns") && description.contains("Stuff."));
    }

    #[test]
    fn container_types_map_to_arrays() {
        let ty: Type = syn::parse_quote!(Option<Vec<[f32; 3]>>);
        assert_eq!(
            type_schema(&ty),
            json!({
                "type": "array",
                "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
            })
        );
        let ty: Type = syn::parse_quote!(serde_json::Value);
        assert_eq!(type_schema(&ty), json!({}));
    }
}
