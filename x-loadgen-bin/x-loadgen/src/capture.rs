//! Response captures: named values read from a response with the same path
//! syntax as assertion checks (`data.items.0.id`), stored with samples and
//! single-probe results, and usable in later requests as `{{captures.<name>}}`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::expect::lookup;

/// Variable prefix that reads a captured value (`{{captures.job_id}}`).
pub const CAPTURE_VAR_PREFIX: &str = "captures.";
/// Most captures one request may declare.
pub const MAX_CAPTURES: usize = 32;
/// Longest capture name.
pub const MAX_NAME_LEN: usize = 64;
/// Longest stored value in bytes; longer values are cut at a character boundary.
pub const MAX_VALUE_BYTES: usize = 1024;
/// Most bytes of values kept from one response; later captures are dropped.
pub const MAX_TOTAL_BYTES: usize = 4096;

/// Captured values by name.
pub type Captures = BTreeMap<String, String>;

/// One capture: store the value at `path` under `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CaptureSpec {
    pub name: String,
    pub path: String,
}

/// Check names and paths of a request's captures.
///
/// # Errors
///
/// Returns a message when there are too many captures, a name is empty, too
/// long, uses characters other than `A-Z a-z 0-9 _ -`, or repeats, or a path is empty.
pub fn validate(specs: &[CaptureSpec]) -> Result<(), String> {
    if specs.len() > MAX_CAPTURES {
        return Err(format!("at most {MAX_CAPTURES} captures per request"));
    }
    let mut seen = Vec::with_capacity(specs.len());
    for spec in specs {
        let name = spec.name.as_str();
        if name.is_empty()
            || name.len() > MAX_NAME_LEN
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        {
            return Err(format!(
                "capture name {name:?} must be 1-{MAX_NAME_LEN} characters of A-Z a-z 0-9 _ -"
            ));
        }
        if seen.contains(&name) {
            return Err(format!("capture name {name:?} is declared twice"));
        }
        seen.push(name);
        if spec.path.trim().is_empty() {
            return Err(format!("capture {name:?} needs a path"));
        }
    }
    Ok(())
}

/// Read every capture that resolves in `body`. Strings are kept as is, other
/// JSON values as compact JSON; `null` and missing paths are skipped.
#[must_use]
pub fn extract(body: Option<&Value>, specs: &[CaptureSpec]) -> Captures {
    let mut out = Captures::new();
    let Some(body) = body else {
        return out;
    };
    let mut total = 0_usize;
    for spec in specs {
        let Some(value) = lookup(body, spec.path.trim()) else {
            continue;
        };
        let text = match value {
            Value::Null => continue,
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let text = truncate(text, MAX_VALUE_BYTES);
        if total + text.len() > MAX_TOTAL_BYTES {
            break;
        }
        total += text.len();
        out.insert(spec.name.clone(), text);
    }
    out
}

/// JSON text of `captures`, `None` when empty (stored in the `captures` column).
#[must_use]
pub fn to_json(captures: &Captures) -> Option<String> {
    (!captures.is_empty())
        .then(|| serde_json::to_string(captures).ok())
        .flatten()
}

fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut cut = max;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    text
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn spec(name: &str, path: &str) -> CaptureSpec {
        CaptureSpec {
            name: name.into(),
            path: path.into(),
        }
    }

    #[test]
    fn extracts_strings_raw_and_other_values_as_json() {
        let body = json!({"job": {"id": "j-1", "n": 3, "tags": ["a"], "none": null}});
        let got = extract(
            Some(&body),
            &[
                spec("job_id", "job.id"),
                spec("n", "job.n"),
                spec("tags", "job.tags"),
                spec("first", "job.tags.0"),
                spec("none", "job.none"),
                spec("missing", "job.nope"),
            ],
        );
        assert_eq!(got.get("job_id").map(String::as_str), Some("j-1"));
        assert_eq!(got.get("n").map(String::as_str), Some("3"));
        assert_eq!(got.get("tags").map(String::as_str), Some(r#"["a"]"#));
        assert_eq!(got.get("first").map(String::as_str), Some("a"));
        assert!(!got.contains_key("none") && !got.contains_key("missing"));
        assert!(extract(None, &[spec("a", "x")]).is_empty());
        assert_eq!(to_json(&Captures::new()), None);
    }

    #[test]
    fn values_and_totals_are_capped() {
        let long = "界".repeat(1000);
        let body = json!({"a": long, "b": long, "c": long, "d": long, "e": long});
        let specs: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|n| spec(n, n))
            .collect();
        let got = extract(Some(&body), &specs);
        assert!(got.values().all(|v| v.len() <= MAX_VALUE_BYTES));
        assert!(got.values().map(String::len).sum::<usize>() <= MAX_TOTAL_BYTES);
        assert_eq!(got.len(), 4);
    }

    #[test]
    fn validation_rejects_bad_names_duplicates_and_empty_paths() {
        assert!(validate(&[spec("job_id", "data.id"), spec("t-2", "x")]).is_ok());
        assert!(validate(&[spec("", "x")]).is_err());
        assert!(validate(&[spec("a.b", "x")]).is_err());
        assert!(validate(&[spec("a", "x"), spec("a", "y")]).is_err());
        assert!(validate(&[spec("a", " ")]).is_err());
        let many: Vec<_> = (0..=MAX_CAPTURES)
            .map(|i| spec(&format!("c{i}"), "x"))
            .collect();
        assert!(validate(&many).is_err());
    }
}
