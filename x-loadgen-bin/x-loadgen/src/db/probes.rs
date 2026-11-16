//! History of interactive single probes (`kind = 'probe'`), newest first,
//! trimmed to the last N entries.

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::error::StoreError;
use super::workspace::now_ms;
use crate::capture::Captures;

/// Default number of probes kept (`--probe-history`).
pub const DEFAULT_PROBE_HISTORY: usize = 1000;
/// Longest stored response body in bytes; longer bodies are cut at a character boundary.
pub const MAX_STORED_BODY_BYTES: usize = 64 * 1024;
/// Default and largest page size of `GET /api/v1/probes`.
pub const DEFAULT_PAGE: u32 = 50;
pub const MAX_PAGE: u32 = 500;

/// One probe to store; text fields must already be scrubbed of secrets.
#[derive(Debug, Clone, Default)]
pub struct NewProbe {
    pub saved_request_id: Option<String>,
    pub request_name: String,
    pub protocol: String,
    pub method: String,
    pub url: String,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub tenant_name: Option<String>,
    pub product_id: Option<String>,
    pub request_id: String,
    pub trace_id: String,
    pub traceparent: String,
    pub trace_echo: String,
    pub http_status: Option<u16>,
    pub transport_ok: bool,
    pub passed_assertions: bool,
    pub latency_us: i64,
    pub response_bytes: usize,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub captures: Captures,
    pub response_body: String,
}

/// A stored probe. `response_body` is only filled by [`get_probe`].
#[derive(Debug, Clone, Serialize)]
pub struct ProbeRecord {
    pub id: i64,
    pub kind: String,
    pub created_at: i64,
    pub saved_request_id: Option<String>,
    pub request_name: String,
    pub protocol: String,
    pub method: String,
    pub url: String,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub tenant_name: Option<String>,
    pub product_id: Option<String>,
    pub request_id: String,
    pub trace_id: String,
    pub traceparent: String,
    pub trace_echo: String,
    pub http_status: Option<i64>,
    pub transport_ok: bool,
    pub passed_assertions: bool,
    pub latency_us: i64,
    pub response_bytes: i64,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub captures: Captures,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    pub response_truncated: bool,
}

/// Filters and keyset cursor of `GET /api/v1/probes`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProbesQuery {
    /// Only probes older than this id (the previous page's `next_before_id`).
    pub before_id: Option<i64>,
    /// Exact `X-Request-Id`.
    pub request_id: Option<String>,
    /// Exact trace-id (case-insensitive).
    pub trace_id: Option<String>,
    /// Probes of one saved request.
    pub saved_request_id: Option<String>,
    pub limit: Option<u32>,
}

const COLUMNS: &str = "id, kind, created_at, saved_request_id, request_name, protocol, method, url,
    environment_id, environment_name, tenant_name, product_id, request_id, trace_id, traceparent,
    trace_echo, http_status, transport_ok, passed_assertions, latency_us, response_bytes,
    error_class, error, captures_json, response_truncated";

/// Store one probe and drop everything but the newest `keep`; returns the new id.
/// `keep == 0` stores nothing and returns `None`.
///
/// # Errors
///
/// Returns a storage error when SQLite fails.
pub fn insert_probe(
    conn: &Connection,
    probe: &NewProbe,
    keep: usize,
) -> Result<Option<i64>, StoreError> {
    if keep == 0 {
        return Ok(None);
    }
    let (body, truncated) = truncate_body(&probe.response_body);
    conn.execute(
        "INSERT INTO probe_history (
            created_at, saved_request_id, request_name, protocol, method, url,
            environment_id, environment_name, tenant_name, product_id, request_id, trace_id,
            traceparent, trace_echo, http_status, transport_ok, passed_assertions, latency_us,
            response_bytes, error_class, error, captures_json, response_body, response_truncated
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                   ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            now_ms()?,
            probe.saved_request_id,
            probe.request_name,
            probe.protocol,
            probe.method,
            probe.url,
            probe.environment_id,
            probe.environment_name,
            probe.tenant_name,
            probe.product_id,
            probe.request_id,
            probe.trace_id,
            probe.traceparent,
            probe.trace_echo,
            probe.http_status,
            i64::from(probe.transport_ok),
            i64::from(probe.passed_assertions),
            probe.latency_us,
            i64::try_from(probe.response_bytes).unwrap_or(i64::MAX),
            probe.error_class,
            probe.error,
            serde_json::to_string(&probe.captures).unwrap_or_else(|_| "{}".into()),
            body,
            i64::from(truncated),
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "DELETE FROM probe_history WHERE id <= (
            SELECT id FROM probe_history ORDER BY id DESC LIMIT 1 OFFSET ?1
         )",
        params![i64::try_from(keep).unwrap_or(i64::MAX)],
    )?;
    Ok(Some(id))
}

/// Newest probes first (without response bodies) and the cursor of the next page.
///
/// # Errors
///
/// Returns a storage error when SQLite fails.
pub fn list_probes(
    conn: &Connection,
    query: &ProbesQuery,
) -> Result<(Vec<ProbeRecord>, Option<i64>), StoreError> {
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let exact = |value: Option<&String>| {
        value
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    };
    let sql = format!(
        "SELECT {COLUMNS} FROM probe_history
         WHERE (?1 IS NULL OR id < ?1)
           AND (?2 IS NULL OR request_id = ?2)
           AND (?3 IS NULL OR trace_id = ?3)
           AND (?4 IS NULL OR saved_request_id = ?4)
         ORDER BY id DESC LIMIT ?5"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut items = stmt
        .query_map(
            params![
                query.before_id,
                exact(query.request_id.as_ref()),
                exact(query.trace_id.as_ref()).map(|id| id.to_ascii_lowercase()),
                exact(query.saved_request_id.as_ref()),
                i64::from(limit) + 1,
            ],
            |row| record_from_row(row, None),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next = if more {
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok((items, next))
}

/// One probe with its stored response body.
///
/// # Errors
///
/// Returns not-found when `id` is unknown.
pub fn get_probe(conn: &Connection, id: i64) -> Result<ProbeRecord, StoreError> {
    conn.query_row(
        &format!("SELECT {COLUMNS}, response_body FROM probe_history WHERE id = ?1"),
        params![id],
        |row| {
            let body: String = row.get(25)?;
            record_from_row(row, Some(body))
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("probe {id} not found")))
}

/// Delete one probe.
///
/// # Errors
///
/// Returns not-found when `id` is unknown.
pub fn delete_probe(conn: &Connection, id: i64) -> Result<(), StoreError> {
    let changed = conn.execute("DELETE FROM probe_history WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(StoreError::not_found(format!("probe {id} not found")));
    }
    Ok(())
}

fn record_from_row(row: &Row<'_>, response_body: Option<String>) -> rusqlite::Result<ProbeRecord> {
    let captures_raw: String = row.get(23)?;
    Ok(ProbeRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        created_at: row.get(2)?,
        saved_request_id: row.get(3)?,
        request_name: row.get(4)?,
        protocol: row.get(5)?,
        method: row.get(6)?,
        url: row.get(7)?,
        environment_id: row.get(8)?,
        environment_name: row.get(9)?,
        tenant_name: row.get(10)?,
        product_id: row.get(11)?,
        request_id: row.get(12)?,
        trace_id: row.get(13)?,
        traceparent: row.get(14)?,
        trace_echo: row.get(15)?,
        http_status: row.get(16)?,
        transport_ok: row.get::<_, i64>(17)? != 0,
        passed_assertions: row.get::<_, i64>(18)? != 0,
        latency_us: row.get(19)?,
        response_bytes: row.get(20)?,
        error_class: row.get(21)?,
        error: row.get(22)?,
        captures: serde_json::from_str(&captures_raw).unwrap_or_default(),
        response_body,
        response_truncated: row.get::<_, i64>(24)? != 0,
    })
}

fn truncate_body(body: &str) -> (&str, bool) {
    if body.len() <= MAX_STORED_BODY_BYTES {
        return (body, false);
    }
    let mut cut = MAX_STORED_BODY_BYTES;
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    (&body[..cut], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(n: usize) -> NewProbe {
        NewProbe {
            request_name: format!("p{n}"),
            protocol: "http".into(),
            method: "GET".into(),
            url: "http://example.test/x".into(),
            request_id: format!("probe-{n}"),
            trace_id: format!("{n:032x}"),
            traceparent: format!("00-{n:032x}-{:016x}-01", n + 1),
            trace_echo: "match".into(),
            ..NewProbe::default()
        }
    }

    #[test]
    fn keeps_the_newest_n_and_pages_newest_first() {
        let dir = std::env::temp_dir().join(format!("xlg-probes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = crate::db::open_central(&crate::db::DataPaths::new(&dir)).expect("db");
        for n in 1..=7 {
            insert_probe(&conn, &probe(n), 5).expect("insert");
        }
        assert_eq!(insert_probe(&conn, &probe(99), 0).expect("off"), None);
        let (page, next) = list_probes(
            &conn,
            &ProbesQuery {
                limit: Some(3),
                ..ProbesQuery::default()
            },
        )
        .expect("list");
        let names: Vec<_> = page.iter().map(|p| p.request_name.as_str()).collect();
        assert_eq!(names, ["p7", "p6", "p5"]);
        assert!(
            page.iter()
                .all(|p| p.response_body.is_none() && p.kind == "probe")
        );
        let (rest, end) = list_probes(
            &conn,
            &ProbesQuery {
                before_id: next,
                ..ProbesQuery::default()
            },
        )
        .expect("page 2");
        let names: Vec<_> = rest.iter().map(|p| p.request_name.as_str()).collect();
        assert_eq!(names, ["p4", "p3"], "only the newest 5 are kept");
        assert_eq!(end, None);
        let (by_trace, _) = list_probes(
            &conn,
            &ProbesQuery {
                trace_id: Some(format!("{:032X}", 6)),
                ..ProbesQuery::default()
            },
        )
        .expect("by trace");
        assert_eq!(by_trace.len(), 1);
        let id = by_trace[0].id;
        assert_eq!(
            get_probe(&conn, id).expect("get").response_body.as_deref(),
            Some("")
        );
        delete_probe(&conn, id).expect("delete");
        assert!(matches!(get_probe(&conn, id), Err(StoreError::NotFound(_))));
        assert!(matches!(
            delete_probe(&conn, id),
            Err(StoreError::NotFound(_))
        ));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_bodies_are_cut_on_a_character_boundary() {
        let body = "界".repeat(30_000);
        let (cut, truncated) = truncate_body(&body);
        assert!(truncated && cut.len() <= MAX_STORED_BODY_BYTES);
        assert!(cut.chars().all(|c| c == '界'));
        assert_eq!(truncate_body("short"), ("short", false));
    }
}
