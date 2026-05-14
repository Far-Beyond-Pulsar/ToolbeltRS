//! `fetch_url` tool — fetch text content from a URL with HTML stripping.

use anyhow::{anyhow, Context};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tool_registry::{tool_params, ChatTool, ToolContext};

pub struct FetchUrlTool;

impl ChatTool for FetchUrlTool {
    fn name(&self) -> &str { "fetch_url" }
    fn category(&self) -> Option<&str> { Some("web") }
    fn description(&self) -> &str {
        "Fetch and return the text content of a URL (HTML markup stripped). \
         Truncates large responses to ~8 000 chars."
    }
    fn parameters_schema(&self) -> Value {
        tool_params! {
            req "url":             string  = "URL to fetch (must start with http:// or https://)",
            opt "timeout_seconds": integer = "Request timeout in seconds (1–30, default 10)"
        }
    }

    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("fetch_url.url is required"))?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(json!({
                "ok": false,
                "url": url,
                "error": "URL must start with http:// or https://",
            }));
        }

        let timeout_secs = args
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(1, 30);

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to build HTTP client")?;

        let resp = match client
            .get(url)
            .header("User-Agent", "ai-tool-registry/1.0")
            .send()
        {
            Ok(r) => r,
            Err(e) => return Ok(json!({ "ok": false, "url": url, "error": e.to_string() })),
        };

        if !resp.status().is_success() {
            return Ok(json!({
                "ok": false, "url": url,
                "status_code": resp.status().as_u16(),
                "error": format!("HTTP {}", resp.status()),
            }));
        }

        let body = match resp.text() {
            Ok(t)  => t,
            Err(e) => return Ok(json!({ "ok": false, "url": url, "error": e.to_string() })),
        };

        let cleaned = strip_html(&body);
        let content = if cleaned.len() > 8_000 {
            format!("{}... [truncated]", &cleaned[..8_000])
        } else {
            cleaned
        };

        Ok(json!({ "ok": true, "url": url, "content_length": content.len(), "content": content }))
    }
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag    = false;
    let mut in_script = false;
    let mut in_style  = false;
    let lower = html.to_lowercase();

    for (i, c) in html.char_indices() {
        match c {
            '<' => {
                in_tag = true;
                if lower[i..].starts_with("<script") { in_script = true; }
                if lower[i..].starts_with("<style")  { in_style  = true; }
            }
            '>' => {
                if lower[i..].starts_with("</script>") { in_script = false; }
                if lower[i..].starts_with("</style>")  { in_style  = false; }
                in_tag = false;
                if !in_script && !in_style { out.push(' '); }
            }
            _ if !in_tag && !in_script && !in_style => out.push(c),
            _ => {}
        }
    }

    out.lines()
       .map(|l| l.trim())
       .filter(|l| !l.is_empty())
       .collect::<Vec<_>>()
       .join("\n")
}
