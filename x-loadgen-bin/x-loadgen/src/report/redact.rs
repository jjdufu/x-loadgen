//! Centralized redaction of credentials in reports and public run views.

use serde_json::Value;

/// JSON object keys that must never appear with their original values in a
/// report or a public run snapshot.
const SECRET_KEYS: &[&str] = &[
    "api_key",
    "tenant_key_base64",
    "binding_key",
    "client_key",
    "access_token",
    "refresh_token",
    "token",
    "password",
    "password_hash",
    "admin_passwd",
    "client_secret",
    "secret",
    "authorization",
    "login_token",
    "session_token",
];

/// Collect plaintext secret values from a snapshot (or any JSON tree) before
/// the keys themselves are stripped.
#[must_use]
pub fn collect_secrets(value: &Value) -> Vec<String> {
    let mut secrets = Vec::new();
    collect_into(value, None, &mut secrets);
    secrets.sort();
    secrets.dedup();
    secrets
}

/// Parse `snapshot_json` and return a tree with secret keys removed and secret
/// values scrubbed from remaining strings. Invalid JSON becomes `null`.
#[must_use]
pub fn redact_snapshot_json(raw: &str) -> Value {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => redact_value(value),
        Err(_) => Value::Null,
    }
}

/// Strip secret keys and mask known secret substrings in `value`.
#[must_use]
pub fn redact_value(value: Value) -> Value {
    let secrets = collect_secrets(&value);
    let mut cleaned = value;
    strip_keys(&mut cleaned);
    scrub_strings(&mut cleaned, &secrets);
    cleaned
}

/// Replace every known secret substring in `text` with a mask.
#[must_use]
pub fn scrub_text(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if !secret.is_empty() && out.contains(secret) {
            out = out.replace(secret, "******");
        }
    }
    out
}

/// Whether `haystack` still contains any collected secret value.
#[must_use]
pub fn contains_any_secret(haystack: &str, secrets: &[String]) -> bool {
    secrets
        .iter()
        .any(|secret| !secret.is_empty() && haystack.contains(secret))
}

fn is_secret_key(key: &str) -> bool {
    SECRET_KEYS
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name))
}

fn collect_into(value: &Value, key: Option<&str>, secrets: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if key.is_some_and(is_secret_key) && !text.is_empty() {
                secrets.push(text.clone());
            }
        }
        Value::Object(map) => {
            for (child_key, child) in map {
                collect_into(child, Some(child_key), secrets);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_into(child, key, secrets);
            }
        }
        _ => {}
    }
}

fn strip_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if is_secret_key(&key) {
                    map.remove(&key);
                } else if let Some(child) = map.get_mut(&key) {
                    strip_keys(child);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_keys(child);
            }
        }
        _ => {}
    }
}

fn scrub_strings(value: &mut Value, secrets: &[String]) {
    if secrets.is_empty() {
        return;
    }
    match value {
        Value::String(text) => {
            *text = scrub_text(text, secrets);
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                scrub_strings(child, secrets);
            }
        }
        Value::Array(items) => {
            for child in items {
                scrub_strings(child, secrets);
            }
        }
        _ => {}
    }
}

/// Remove secret keys from an already-built report and scrub leftover strings
/// using secrets collected from the original snapshot.
pub fn apply_secrets(report: &mut Value, secrets: &[String]) {
    strip_keys(report);
    scrub_strings(report, secrets);
    if let Some(error) = object_string_mut(report, "error") {
        *error = scrub_text(error, secrets);
    }
}

fn object_string_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut String> {
    match value {
        Value::Object(map) => match map.get_mut(key) {
            Some(Value::String(text)) => Some(text),
            _ => None,
        },
        _ => None,
    }
}
