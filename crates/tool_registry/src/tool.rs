//! [`ChatTool`] trait and [`ToolDefinition`] metadata.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single AI-callable tool.
///
/// Implement this trait on a zero-sized struct and register it with [`ToolRegistry`].
/// All methods are `&self` so the struct can be `Arc`-shared across threads.
pub trait ChatTool: Send + Sync {
    /// Short, unique snake_case name used by the LLM to invoke this tool.
    fn name(&self) -> &'static str;

    /// Human-readable description sent to the LLM in the system prompt / tool list.
    fn description(&self) -> &'static str;

    /// JSON Schema object describing the accepted `arguments` payload.
    ///
    /// Minimum valid value: `json!({"type": "object", "properties": {}})`.
    fn parameters_schema(&self) -> Value;

    /// Optional category for grouping tools in the system prompt.
    fn category(&self) -> Option<&'static str> {
        None
    }

    /// Execute the tool with the provided arguments and caller context.
    fn execute(
        &self,
        args: Value,
        ctx: &crate::ToolContext,
    ) -> anyhow::Result<Value>;

    /// Return the [`ToolDefinition`] metadata derived from this tool's trait methods.
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: self.parameters_schema(),
            category: self.category().map(|s| s.to_string()),
        }
    }
}

/// Serialisable metadata for a single tool — used when building LLM request payloads.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}
