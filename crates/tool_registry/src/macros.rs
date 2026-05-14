//! The `tool_params!` declarative macro — build JSON Schema parameter objects inline.
//!
//! # Example
//!
//! ```rust
//! use tool_registry::tool_params;
//!
//! let schema = tool_params! {
//!     req "query":   string  = "Search query string",
//!     opt "limit":   integer = "Maximum number of results",
//!     opt "verbose": boolean = "Include extra detail",
//! };
//!
//! assert_eq!(schema["required"][0], "query");
//! assert_eq!(schema["properties"]["limit"]["type"], "integer");
//! ```

/// Build a JSON Schema `parameters` object from a list of typed fields.
///
/// Syntax: `req` marks a required parameter; `opt` marks an optional one.
/// Supported JSON types: `string`, `integer`, `number`, `boolean`, `object`, `array`.
///
/// ```rust
/// # use tool_registry::tool_params;
/// let schema = tool_params! {
///     req "file_path": string = "Path to the file",
///     opt "dry_run":  boolean = "Preview changes without applying",
/// };
/// ```
#[macro_export]
macro_rules! tool_params {
    ( $( $req:ident $name:literal : $ty:ident = $desc:literal ),* $(,)? ) => {{
        let mut properties = serde_json::Map::new();
        let mut required: Vec<serde_json::Value> = Vec::new();
        $(
            properties.insert(
                $name.to_string(),
                serde_json::json!({ "type": stringify!($ty), "description": $desc }),
            );
            $crate::tool_params!(@push $req required $name);
        )*
        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    }};
    () => {
        serde_json::json!({ "type": "object", "properties": {} })
    };
    // Internal: push name into `required` if marked `req`, skip if `opt`.
    (@push req $vec:ident $name:literal) => {
        $vec.push(serde_json::json!($name));
    };
    (@push opt $vec:ident $name:literal) => {};
}
