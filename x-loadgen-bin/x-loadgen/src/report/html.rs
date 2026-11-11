//! Standalone Chinese HTML report (single file, no external requests).

use serde_json::Value;

const ECHARTS: &str = include_str!("../../assets/echarts.min.js");
const PAGE: &str = include_str!("page.html");

/// Render a redacted report JSON value as a standalone HTML document.
#[must_use]
pub fn render(report: &Value) -> String {
    let payload = safe_json(report);
    let id = escape_attr(
        report
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    );
    let status = escape_attr(report.get("status").and_then(Value::as_str).unwrap_or("-"));
    PAGE.replace("__ID__", &id)
        .replace("__STATUS__", &status)
        .replace("__PAYLOAD__", &payload)
        .replace("__ECHARTS__", ECHARTS)
}

fn safe_json(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "null".to_string())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
