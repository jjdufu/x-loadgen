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
    replace_once_all(
        PAGE,
        &[
            ("__ID__", id.as_str()),
            ("__STATUS__", status.as_str()),
            ("__PAYLOAD__", payload.as_str()),
            ("__ECHARTS__", ECHARTS),
        ],
    )
}

fn replace_once_all(template: &str, tokens: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + ECHARTS.len());
    let mut rest = template;
    while !rest.is_empty() {
        let mut earliest: Option<(usize, &str, &str)> = None;
        for &(needle, replacement) in tokens {
            if let Some(pos) = rest.find(needle)
                && earliest.is_none_or(|(best, _, _)| pos < best)
            {
                earliest = Some((pos, needle, replacement));
            }
        }
        if let Some((pos, needle, replacement)) = earliest {
            out.push_str(&rest[..pos]);
            out.push_str(replacement);
            rest = &rest[pos + needle.len()..];
        } else {
            out.push_str(rest);
            break;
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::render;

    #[test]
    fn payload_containing_placeholder_tokens_is_not_reexpanded() {
        let report = json!({
            "id": "run-1",
            "status": "finished",
            "title": "literal __ECHARTS__ and __PAYLOAD__ marker",
        });
        let html = render(&report);
        assert!(html.contains("\"title\":\"literal __ECHARTS__ and __PAYLOAD__ marker\""));
    }
}
