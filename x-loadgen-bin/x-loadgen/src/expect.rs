//! Business checks on the gateway success payload (`data`), not on the HTTP envelope.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FieldCheck {
    pub path: String,
    /// Absent means the path must exist and not be null.
    #[serde(default)]
    pub eq: Option<Value>,
}

impl FieldCheck {
    #[cfg(test)]
    pub fn eq(path: impl Into<String>, value: Value) -> Self {
        Self {
            path: path.into(),
            eq: Some(value),
        }
    }

    #[cfg(test)]
    pub fn exists(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            eq: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportExpect {
    Ok,
    Reject,
}

impl TransportExpect {
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ok" => Some(Self::Ok),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judge {
    Met,
    /// Expectation was transport success, and the gateway call itself failed.
    TransportFailed,
    Failed {
        class: &'static str,
        message: String,
    },
}

#[must_use]
pub fn judge(
    transport_ok: bool,
    data: Option<&Value>,
    transport: TransportExpect,
    checks: &[FieldCheck],
) -> Judge {
    match transport {
        TransportExpect::Reject => {
            if transport_ok {
                Judge::Failed {
                    class: "unexpected_accept",
                    message: "expected the gateway to reject this call".into(),
                }
            } else {
                Judge::Met
            }
        }
        TransportExpect::Ok => {
            if !transport_ok {
                return Judge::TransportFailed;
            }
            let payload = data.unwrap_or(&Value::Null);
            for check in checks {
                if let Err(message) = eval_check(payload, check) {
                    return Judge::Failed {
                        class: "business",
                        message,
                    };
                }
            }
            Judge::Met
        }
    }
}

fn eval_check(payload: &Value, check: &FieldCheck) -> Result<(), String> {
    let found = lookup(payload, &check.path);
    match &check.eq {
        None => match found {
            Some(value) if !value.is_null() => Ok(()),
            _ => Err(format!("missing {}", check.path)),
        },
        Some(expected) => {
            let Some(got) = found else {
                return Err(format!("missing {}", check.path));
            };
            if json_eq(got, expected) {
                Ok(())
            } else {
                Err(format!("{} = {got} , expected {expected}", check.path))
            }
        }
    }
}

#[must_use]
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    if path.is_empty() {
        return None;
    }
    for seg in path.split('.') {
        if seg.is_empty() {
            return None;
        }
        cur = if let Ok(index) = seg.parse::<usize>() {
            cur.get(index)?
        } else {
            cur.get(seg)?
        };
    }
    Some(cur)
}

fn json_eq(got: &Value, expected: &Value) -> bool {
    if got == expected {
        return true;
    }
    match (got.as_i64(), expected.as_i64()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lookup_walks_objects_and_indexes() {
        let value = json!({"items":[{"id":18}],"total":1});
        assert_eq!(lookup(&value, "total"), Some(&json!(1)));
        assert_eq!(lookup(&value, "items.0.id"), Some(&json!(18)));
        assert!(lookup(&value, "items.1.id").is_none());
        assert!(lookup(&value, "").is_none());
    }

    #[test]
    fn ok_requires_checks_mismatch_is_not_transport_failure() {
        let data = json!({"matched": false, "distance": 64});
        let checks = vec![
            FieldCheck::eq("matched", json!(false)),
            FieldCheck::eq("distance", json!(64)),
        ];
        assert_eq!(
            judge(true, Some(&data), TransportExpect::Ok, &checks),
            Judge::Met
        );
        let wrong = json!({"matched": true, "distance": 0});
        assert!(matches!(
            judge(true, Some(&wrong), TransportExpect::Ok, &checks),
            Judge::Failed {
                class: "business",
                ..
            }
        ));
    }

    #[test]
    fn reject_is_met_only_when_transport_fails() {
        assert_eq!(judge(false, None, TransportExpect::Reject, &[]), Judge::Met);
        assert!(matches!(
            judge(true, Some(&json!({})), TransportExpect::Reject, &[]),
            Judge::Failed {
                class: "unexpected_accept",
                ..
            }
        ));
        assert_eq!(
            judge(false, None, TransportExpect::Ok, &[]),
            Judge::TransportFailed
        );
    }

    #[test]
    fn exists_rejects_null() {
        let data = json!({"image":{"base64": null}});
        let checks = vec![FieldCheck::exists("image.base64")];
        assert!(matches!(
            judge(true, Some(&data), TransportExpect::Ok, &checks),
            Judge::Failed {
                class: "business",
                ..
            }
        ));
    }
}
