//! Hoppscotch-compatible variable interpolation (`<<var>>` and `{{var}}`)
//! plus load-test runtime tokens (`seq`, `$timestamp`, `$timestampMs`,
//! `$randomUUID`, `$randomInt`) and response captures (`captures.<name>`).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::capture::{CAPTURE_VAR_PREFIX, Captures};

/// Per-request runtime tokens (`{{seq}}` / `<<seq>>`, `$randomUUID`, etc.).
#[derive(Debug, Clone)]
pub struct RuntimeVars {
    /// Monotonic 0-based request index (`{{seq}}` / `<<seq>>`).
    pub seq: u64,
    /// Pre-generated `UUIDv4` for this request execution.
    pub uuid: String,
    /// Latest captured values (`{{captures.<name>}}`).
    pub captures: Arc<Captures>,
}

impl RuntimeVars {
    /// Construct runtime tokens for request index `seq`.
    #[must_use]
    pub fn for_index(seq: u64) -> Self {
        Self {
            seq,
            uuid: Uuid::new_v4().to_string(),
            captures: Arc::default(),
        }
    }

    /// Same tokens with `captures` readable as `{{captures.<name>}}`.
    #[must_use]
    pub fn with_captures(mut self, captures: Arc<Captures>) -> Self {
        self.captures = captures;
        self
    }
}

/// Interpolate both Hoppscotch `<<name>>` and mustache `{{name}}` placeholders
/// inside `template`. Unknown variables are left untouched so downstream
/// debugging can inspect them.
#[must_use]
pub fn render_str(
    template: &str,
    vars: &BTreeMap<String, String>,
    runtime: &RuntimeVars,
) -> String {
    if !template.contains("<<") && !template.contains("{{") {
        return template.to_string();
    }
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;
    while i < len {
        if i + 3 < len
            && bytes[i] == b'<'
            && bytes[i + 1] == b'<'
            && let Some(end) = find_closing(template, i + 2, ">>")
        {
            let raw_name = &template[i + 2..end];
            let trimmed = raw_name.trim();
            if let Some(val) = resolve_token(trimmed, vars, runtime) {
                out.push_str(&val);
            } else {
                out.push_str(&template[i..end + 2]);
            }
            i = end + 2;
            continue;
        }
        if i + 3 < len
            && bytes[i] == b'{'
            && bytes[i + 1] == b'{'
            && let Some(end) = find_closing(template, i + 2, "}}")
        {
            let raw_name = &template[i + 2..end];
            let trimmed = raw_name.trim();
            if let Some(val) = resolve_token(trimmed, vars, runtime) {
                out.push_str(&val);
            } else {
                out.push_str(&template[i..end + 2]);
            }
            i = end + 2;
            continue;
        }
        let ch = template[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Return any unresolved variable names referenced via `<<name>>` or `{{name}}`.
#[must_use]
pub fn unresolved_tokens(
    template: &str,
    vars: &BTreeMap<String, String>,
    runtime: &RuntimeVars,
) -> Vec<String> {
    let mut missing = Vec::new();
    for token in scan_tokens(template) {
        if resolve_token(&token, vars, runtime).is_none() && !missing.contains(&token) {
            missing.push(token);
        }
    }
    missing
}

fn scan_tokens(template: &str) -> Vec<String> {
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut found = Vec::new();
    let mut i = 0;
    while i < len {
        if i + 3 < len
            && bytes[i] == b'<'
            && bytes[i + 1] == b'<'
            && let Some(end) = find_closing(template, i + 2, ">>")
        {
            let name = template[i + 2..end].trim();
            if is_valid_token_name(name) {
                found.push(name.to_string());
            }
            i = end + 2;
            continue;
        }
        if i + 3 < len
            && bytes[i] == b'{'
            && bytes[i + 1] == b'{'
            && let Some(end) = find_closing(template, i + 2, "}}")
        {
            let name = template[i + 2..end].trim();
            if is_valid_token_name(name) {
                found.push(name.to_string());
            }
            i = end + 2;
            continue;
        }
        let ch = template[i..].chars().next().unwrap_or('\0');
        i += ch.len_utf8();
    }
    found
}

fn find_closing(text: &str, start: usize, closing: &str) -> Option<usize> {
    let rest = &text[start..];
    let rel = rest.find(closing)?;
    let inner = &rest[..rel];
    if inner.contains('\n') || inner.contains('\r') {
        return None;
    }
    Some(start + rel)
}

fn is_valid_token_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '$'))
}

fn resolve_token(
    name: &str,
    vars: &BTreeMap<String, String>,
    runtime: &RuntimeVars,
) -> Option<String> {
    if !is_valid_token_name(name) {
        return None;
    }
    match name {
        "seq" | "$seq" => Some(runtime.seq.to_string()),
        "$timestamp" => {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            Some(secs.to_string())
        }
        "$timestampMs" | "$isoTimestamp" => {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
            Some(ms.to_string())
        }
        "$randomUUID" | "$guid" => Some(runtime.uuid.clone()),
        "$randomInt" => {
            let mixed = runtime
                .seq
                .wrapping_add(1)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| u64::from(d.subsec_nanos()));
            Some((mixed % 1000).to_string())
        }
        _ => match name.strip_prefix(CAPTURE_VAR_PREFIX) {
            Some(capture) => runtime.captures.get(capture).cloned(),
            None => vars.get(name).cloned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{RuntimeVars, render_str, unresolved_tokens};

    #[test]
    fn renders_hoppscotch_and_mustache_and_dynamic_tokens() {
        let mut vars = BTreeMap::new();
        vars.insert("baseUrl".into(), "https://api.example.com".into());
        vars.insert("api_version".into(), "v2".into());
        let rt = RuntimeVars::for_index(42);
        let out = render_str(
            "<<baseUrl>>/{{api_version}}/items/{{seq}}?uid=<<$randomUUID>>",
            &vars,
            &rt,
        );
        assert!(out.starts_with("https://api.example.com/v2/items/42?uid="));
        assert_eq!(
            out.len(),
            "https://api.example.com/v2/items/42?uid=".len() + 36
        );
    }

    #[test]
    fn preserves_json_braces_and_reports_unresolved() {
        let vars = BTreeMap::new();
        let raw = r#"{"id": {{seq}}, "missing": "<<unknownVar>>", "nested": {"a": 1}}"#;
        let rt = RuntimeVars::for_index(7);
        let out = render_str(raw, &vars, &rt);
        assert_eq!(
            out,
            r#"{"id": 7, "missing": "<<unknownVar>>", "nested": {"a": 1}}"#
        );
        assert_eq!(
            unresolved_tokens(raw, &vars, &rt),
            vec!["unknownVar".to_string()]
        );
    }

    #[test]
    fn captures_resolve_only_from_the_runtime_map() {
        let mut vars = BTreeMap::new();
        vars.insert("captures.job_id".into(), "from-env".into());
        let mut captured = crate::capture::Captures::new();
        captured.insert("job_id".into(), "j-9".into());
        let rt = RuntimeVars::for_index(0).with_captures(std::sync::Arc::new(captured));
        assert_eq!(
            render_str("/jobs/{{captures.job_id}}", &vars, &rt),
            "/jobs/j-9"
        );
        let empty = RuntimeVars::for_index(0);
        assert_eq!(
            render_str("/jobs/<<captures.job_id>>", &vars, &empty),
            "/jobs/<<captures.job_id>>"
        );
        assert_eq!(
            unresolved_tokens("{{captures.job_id}}", &vars, &empty),
            vec!["captures.job_id".to_string()]
        );
    }
}
