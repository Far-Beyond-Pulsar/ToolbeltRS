//! `web_search` tool — DuckDuckGo HTML scrape, up to 10 results.

use anyhow::{anyhow, Context};
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::time::Duration;
use tool_registry::{tool_params, ChatTool, ToolContext};

pub struct WebSearchTool;

impl ChatTool for WebSearchTool {
    fn name(&self) -> &'static str { "web_search" }
    fn category(&self) -> Option<&'static str> { Some("web") }
    fn description(&self) -> &'static str {
        "Search the web via DuckDuckGo. Returns up to 10 results with title, summary, and URL."
    }
    fn parameters_schema(&self) -> Value {
        tool_params! { req "query": string = "Search query string" }
    }

    fn execute(&self, args: Value, _ctx: &ToolContext) -> anyhow::Result<Value> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("web_search.query is required"))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let url = format!(
            "https://html.duckduckgo.com/html/?q={}&kl=us-en",
            urlencoding::encode(query)
        );

        let response = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 ai-tool-registry/1.0")
            .send()
            .context("Failed to perform web search")?;

        if !response.status().is_success() {
            return Ok(json!({
                "ok": false,
                "query": query,
                "error": format!("Search page returned status {}", response.status()),
                "results": []
            }));
        }

        let body = response.text().context("Failed to read search response")?;
        let results = parse_results(&body, 10)?;

        Ok(json!({
            "ok": true,
            "query": query,
            "result_count": results.len(),
            "results": results,
        }))
    }
}

fn parse_results(html: &str, max: usize) -> anyhow::Result<Vec<Value>> {
    let doc = Html::parse_document(html);
    let result_sel  = sel("div.result")?;
    let title_sel   = sel("a.result__a")?;
    let snippet_sel = sel("a.result__snippet, div.result__snippet")?;

    let mut out = Vec::new();
    for result in doc.select(&result_sel) {
        if out.len() >= max { break; }
        let Some(title_el) = result.select(&title_sel).next() else { continue; };
        let title = normalise_text(title_el.text().collect::<Vec<_>>().join(" "));
        if title.is_empty() { continue; }
        let href = title_el.value().attr("href").unwrap_or("");
        let url  = normalise_ddg_url(href);
        if url.is_empty() { continue; }
        let summary = result
            .select(&snippet_sel)
            .next()
            .map(|el| normalise_text(el.text().collect::<Vec<_>>().join(" ")))
            .filter(|t| !t.is_empty())
            .map(|t| truncate(&t, 300))
            .unwrap_or_else(|| title.clone());
        out.push(json!({ "title": title, "summary": summary, "url": url }));
    }
    Ok(out)
}

fn sel(css: &str) -> anyhow::Result<Selector> {
    Selector::parse(css).map_err(|_| anyhow!("Invalid selector: {css}"))
}

fn normalise_text(s: String) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let mut end = max;
    while !s.is_char_boundary(end) { end -= 1; }
    format!("{}...", &s[..end])
}

fn normalise_ddg_url(href: &str) -> String {
    if let Some(enc) = href.split("uddg=").nth(1) {
        let enc = enc.split('&').next().unwrap_or(enc);
        return urlencoding::decode(enc).map(|d| d.into_owned()).unwrap_or_else(|_| href.to_string());
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    String::new()
}
