//! Persistence for load-test templates.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::error::StoreError;
use super::targets::{now_ms, target_exists, unfinished_run_count};

type ProductFilter = (Option<Vec<String>>, Option<HashMap<String, i64>>);

/// Template fields accepted by create and full update.
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateInput {
    pub name: String,
    pub target_id: String,
    pub model: String,
    #[serde(default)]
    pub qps: Option<i64>,
    pub concurrency: i64,
    #[serde(default)]
    pub requests: Option<i64>,
    #[serde(default)]
    pub duration_secs: Option<i64>,
    #[serde(default)]
    pub warmup_secs: i64,
    #[serde(default)]
    pub ramp_secs: i64,
    pub timeout_secs: i64,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: i64,
    #[serde(default = "default_token_skew")]
    pub token_refresh_skew_secs: i64,
    #[serde(default = "default_true")]
    pub include_request_id: bool,
    #[serde(default = "default_processes")]
    pub processes: i64,
    #[serde(default = "default_mode_total")]
    pub qps_mode: String,
    #[serde(default = "default_mode_total")]
    pub requests_mode: String,
    #[serde(default)]
    pub loop_enabled: bool,
    #[serde(default)]
    pub rounds: Option<i64>,
    #[serde(default)]
    pub loop_interval_secs: i64,
    #[serde(default)]
    pub expected_reasons: Option<Vec<String>>,
    #[serde(default)]
    pub detail_retain_days: Option<i64>,
    #[serde(default)]
    pub detail_retain_rows: Option<i64>,
    #[serde(default)]
    pub product_ids: Option<Vec<String>>,
    #[serde(default)]
    pub weight_overrides: Option<HashMap<String, i64>>,
    #[serde(default)]
    pub note: Option<String>,
}

fn default_connect_timeout() -> i64 {
    5
}

fn default_token_skew() -> i64 {
    60
}

fn default_true() -> bool {
    true
}

fn default_processes() -> i64 {
    1
}

fn default_mode_total() -> String {
    "total".to_string()
}

/// Stored template as returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub target_id: String,
    pub model: String,
    pub qps: Option<i64>,
    pub concurrency: i64,
    pub requests: Option<i64>,
    pub duration_secs: Option<i64>,
    pub warmup_secs: i64,
    pub ramp_secs: i64,
    pub timeout_secs: i64,
    pub connect_timeout_secs: i64,
    pub token_refresh_skew_secs: i64,
    pub include_request_id: bool,
    pub processes: i64,
    pub qps_mode: String,
    pub requests_mode: String,
    pub loop_enabled: bool,
    pub rounds: Option<i64>,
    pub loop_interval_secs: i64,
    pub expected_reasons: Option<Vec<String>>,
    pub detail_retain_days: Option<i64>,
    pub detail_retain_rows: Option<i64>,
    pub product_ids: Option<Vec<String>>,
    pub weight_overrides: Option<HashMap<String, i64>>,
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// List all templates.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn list_templates(conn: &Connection) -> Result<Vec<Template>, StoreError> {
    let mut stmt = conn.prepare("SELECT * FROM templates ORDER BY name")?;
    let rows = stmt.query_map([], template_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Load one template.
///
/// # Errors
///
/// Returns not-found when the id is unknown.
pub fn get_template(conn: &Connection, id: &str) -> Result<Template, StoreError> {
    conn.query_row(
        "SELECT * FROM templates WHERE id = ?1",
        params![id],
        template_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("template {id} not found")))
}

/// Load a template by its unique name.
///
/// # Errors
///
/// Returns not-found when the name is unknown.
pub fn get_template_by_name(conn: &Connection, name: &str) -> Result<Template, StoreError> {
    conn.query_row(
        "SELECT * FROM templates WHERE name = ?1",
        params![name],
        template_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("template '{name}' not found")))
}

/// Load a template by id, falling back to a unique name lookup.
///
/// # Errors
///
/// Returns not-found when neither the id nor the name matches.
pub fn get_template_by_id_or_name(conn: &Connection, key: &str) -> Result<Template, StoreError> {
    match get_template(conn, key) {
        Ok(template) => Ok(template),
        Err(StoreError::NotFound(_)) => get_template_by_name(conn, key),
        Err(err) => Err(err),
    }
}

/// Re-validate a template after in-memory overrides have been applied.
///
/// # Errors
///
/// Returns a validation, not-found (missing target), or storage error.
pub fn validate_effective(
    conn: &Connection,
    template: &Template,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<(), StoreError> {
    validate_template(conn, &template_to_input(template), max_qps, max_concurrency)
}

fn template_to_input(template: &Template) -> TemplateInput {
    TemplateInput {
        name: template.name.clone(),
        target_id: template.target_id.clone(),
        model: template.model.clone(),
        qps: template.qps,
        concurrency: template.concurrency,
        requests: template.requests,
        duration_secs: template.duration_secs,
        warmup_secs: template.warmup_secs,
        ramp_secs: template.ramp_secs,
        timeout_secs: template.timeout_secs,
        connect_timeout_secs: template.connect_timeout_secs,
        token_refresh_skew_secs: template.token_refresh_skew_secs,
        include_request_id: template.include_request_id,
        processes: template.processes,
        qps_mode: template.qps_mode.clone(),
        requests_mode: template.requests_mode.clone(),
        loop_enabled: template.loop_enabled,
        rounds: template.rounds,
        loop_interval_secs: template.loop_interval_secs,
        expected_reasons: template.expected_reasons.clone(),
        detail_retain_days: template.detail_retain_days,
        detail_retain_rows: template.detail_retain_rows,
        product_ids: template.product_ids.clone(),
        weight_overrides: template.weight_overrides.clone(),
        note: template.note.clone(),
    }
}

/// Insert a template after validation (including optional global caps).
///
/// # Errors
///
/// Returns validation, conflict, or storage errors.
pub fn create_template(
    conn: &Connection,
    input: &TemplateInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<Template, StoreError> {
    validate_template(conn, input, max_qps, max_concurrency)?;
    if template_name_taken(conn, &input.name, None)? {
        return Err(StoreError::conflict(format!(
            "a template named '{}' already exists",
            input.name
        )));
    }
    let id = super::targets::new_id();
    let now = now_ms()?;
    insert_template_row(conn, &id, input, now, now)?;
    get_template(conn, &id)
}

/// Replace a template.
///
/// # Errors
///
/// Returns validation, not-found, conflict, or storage errors.
pub fn update_template(
    conn: &Connection,
    id: &str,
    input: &TemplateInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<Template, StoreError> {
    let existing = get_template(conn, id)?;
    validate_template(conn, input, max_qps, max_concurrency)?;
    if template_name_taken(conn, &input.name, Some(id))? {
        return Err(StoreError::conflict(format!(
            "a template named '{}' already exists",
            input.name
        )));
    }
    let now = now_ms()?;
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
    insert_template_row(conn, id, input, existing.created_at, now)?;
    get_template(conn, id)
}

/// Delete a template unless an unfinished run still references it.
///
/// # Errors
///
/// Returns not-found, conflict, or storage errors.
pub fn delete_template(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let _ = get_template(conn, id)?;
    if unfinished_run_count(conn, "template_id", id)? > 0 {
        return Err(StoreError::conflict(
            "the template is still referenced by an unfinished run",
        ));
    }
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
    Ok(())
}

fn insert_template_row(
    conn: &Connection,
    id: &str,
    input: &TemplateInput,
    created_at: i64,
    updated_at: i64,
) -> Result<(), StoreError> {
    let expected = input
        .expected_reasons
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let filter =
        encode_product_filter(input.product_ids.as_ref(), input.weight_overrides.as_ref())?;
    conn.execute(
        "INSERT INTO templates (
            id, name, target_id, model, qps, concurrency, requests, duration_secs,
            warmup_secs, ramp_secs, timeout_secs, connect_timeout_secs,
            token_refresh_skew_secs, include_request_id, processes, qps_mode,
            requests_mode, loop_enabled, rounds, loop_interval_secs,
            expected_reasons_json, detail_retain_days, detail_retain_rows,
            product_filter_json, note, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
         )",
        params![
            id,
            input.name.trim(),
            input.target_id,
            input.model.trim(),
            input.qps,
            input.concurrency,
            input.requests,
            input.duration_secs,
            input.warmup_secs,
            input.ramp_secs,
            input.timeout_secs,
            input.connect_timeout_secs,
            input.token_refresh_skew_secs,
            i64::from(input.include_request_id),
            input.processes,
            input.qps_mode.trim(),
            input.requests_mode.trim(),
            i64::from(input.loop_enabled),
            input.rounds,
            input.loop_interval_secs,
            expected,
            input.detail_retain_days,
            input.detail_retain_rows,
            filter,
            input
                .note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            created_at,
            updated_at,
        ],
    )?;
    Ok(())
}

fn template_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Template> {
    let expected = parse_string_list(row.get(20)?)?;
    let (product_ids, weight_overrides) = parse_product_filter(row.get(23)?)?;
    Ok(Template {
        id: row.get(0)?,
        name: row.get(1)?,
        target_id: row.get(2)?,
        model: row.get(3)?,
        qps: row.get(4)?,
        concurrency: row.get(5)?,
        requests: row.get(6)?,
        duration_secs: row.get(7)?,
        warmup_secs: row.get(8)?,
        ramp_secs: row.get(9)?,
        timeout_secs: row.get(10)?,
        connect_timeout_secs: row.get(11)?,
        token_refresh_skew_secs: row.get(12)?,
        include_request_id: row.get::<_, i64>(13)? != 0,
        processes: row.get(14)?,
        qps_mode: row.get(15)?,
        requests_mode: row.get(16)?,
        loop_enabled: row.get::<_, i64>(17)? != 0,
        rounds: row.get(18)?,
        loop_interval_secs: row.get(19)?,
        expected_reasons: expected,
        detail_retain_days: row.get(21)?,
        detail_retain_rows: row.get(22)?,
        product_ids,
        weight_overrides,
        note: row.get(24)?,
        created_at: row.get(25)?,
        updated_at: row.get(26)?,
    })
}

fn parse_string_list(raw: Option<String>) -> rusqlite::Result<Option<Vec<String>>> {
    let Some(text) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    serde_json::from_str(&text).map(Some).map_err(json_row_err)
}

fn parse_product_filter(raw: Option<String>) -> rusqlite::Result<ProductFilter> {
    let Some(text) = raw.filter(|s| !s.is_empty()) else {
        return Ok((None, None));
    };
    let value: Value = serde_json::from_str(&text).map_err(json_row_err)?;
    let product_ids = value
        .get("product_ids")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(json_row_err)?;
    let weight_overrides = value
        .get("weight_overrides")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(json_row_err)?;
    Ok((product_ids, weight_overrides))
}

fn encode_product_filter(
    product_ids: Option<&Vec<String>>,
    weight_overrides: Option<&HashMap<String, i64>>,
) -> Result<Option<String>, StoreError> {
    if product_ids.is_none() && weight_overrides.is_none() {
        return Ok(None);
    }
    let mut map = Map::new();
    if let Some(ids) = product_ids {
        map.insert("product_ids".to_string(), serde_json::to_value(ids)?);
    }
    if let Some(weights) = weight_overrides {
        map.insert(
            "weight_overrides".to_string(),
            serde_json::to_value(weights)?,
        );
    }
    Ok(Some(Value::Object(map).to_string()))
}

fn json_row_err(err: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
}

fn validate_template(
    conn: &Connection,
    input: &TemplateInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<(), StoreError> {
    if input.name.trim().is_empty() {
        return Err(StoreError::invalid("template name is required"));
    }
    if !target_exists(conn, &input.target_id)? {
        return Err(StoreError::not_found(format!(
            "target {} not found",
            input.target_id
        )));
    }
    match input.model.trim() {
        "open_loop" | "closed_loop" => {}
        _ => {
            return Err(StoreError::invalid(
                "model must be open_loop or closed_loop",
            ));
        }
    }
    validate_window(input)?;
    validate_caps(input, max_qps, max_concurrency)?;
    validate_mode("qps_mode", &input.qps_mode)?;
    validate_mode("requests_mode", &input.requests_mode)?;
    validate_product_filter(conn, &input.target_id, input)?;
    Ok(())
}

fn validate_window(input: &TemplateInput) -> Result<(), StoreError> {
    if input.concurrency < 1 {
        return Err(StoreError::invalid("concurrency must be at least 1"));
    }
    if input.requests.is_some() == input.duration_secs.is_some() {
        return Err(StoreError::invalid(
            "exactly one of requests or duration_secs must be set and greater than 0",
        ));
    }
    if let Some(requests) = input.requests
        && requests <= 0
    {
        return Err(StoreError::invalid("requests must be greater than 0"));
    }
    if let Some(duration) = input.duration_secs
        && duration <= 0
    {
        return Err(StoreError::invalid("duration_secs must be greater than 0"));
    }
    if input.timeout_secs <= 0 {
        return Err(StoreError::invalid("timeout_secs must be greater than 0"));
    }
    if input.connect_timeout_secs <= 0 {
        return Err(StoreError::invalid(
            "connect_timeout_secs must be greater than 0",
        ));
    }
    if input.token_refresh_skew_secs < 0 {
        return Err(StoreError::invalid(
            "token_refresh_skew_secs must be at least 0",
        ));
    }
    if input.processes < 1 {
        return Err(StoreError::invalid("processes must be at least 1"));
    }
    if input.warmup_secs < 0 || input.ramp_secs < 0 || input.loop_interval_secs < 0 {
        return Err(StoreError::invalid(
            "warmup_secs, ramp_secs, and loop_interval_secs must be at least 0",
        ));
    }
    if let Some(rounds) = input.rounds
        && rounds < 1
    {
        return Err(StoreError::invalid(
            "rounds must be at least 1 when set (omit or use null for an infinite loop)",
        ));
    }
    if let Some(days) = input.detail_retain_days
        && days < 1
    {
        return Err(StoreError::invalid(
            "detail_retain_days must be at least 1 when set",
        ));
    }
    if let Some(rows) = input.detail_retain_rows
        && rows < 1
    {
        return Err(StoreError::invalid(
            "detail_retain_rows must be at least 1 when set",
        ));
    }
    Ok(())
}

fn validate_caps(
    input: &TemplateInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<(), StoreError> {
    if let Some(qps) = input.qps {
        if qps <= 0 {
            return Err(StoreError::invalid("qps must be greater than 0 when set"));
        }
        if let Some(max) = max_qps
            && u64::try_from(qps).unwrap_or(u64::MAX) > max
        {
            return Err(StoreError::invalid(format!(
                "template qps {qps} exceeds the global cap of {max}"
            )));
        }
    }
    if let Some(max) = max_concurrency
        && u64::try_from(input.concurrency).unwrap_or(u64::MAX) > max
    {
        return Err(StoreError::invalid(format!(
            "template concurrency {} exceeds the global cap of {max}",
            input.concurrency
        )));
    }
    Ok(())
}

fn validate_mode(field: &str, value: &str) -> Result<(), StoreError> {
    match value.trim() {
        "total" | "per_process" => Ok(()),
        _ => Err(StoreError::invalid(format!(
            "{field} must be total or per_process"
        ))),
    }
}

fn validate_product_filter(
    conn: &Connection,
    target_id: &str,
    input: &TemplateInput,
) -> Result<(), StoreError> {
    let known: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM target_products WHERE target_id = ?1")?;
        let rows = stmt.query_map(params![target_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if let Some(ids) = &input.product_ids {
        for id in ids {
            if !known.iter().any(|known_id| known_id == id) {
                return Err(StoreError::invalid(format!(
                    "product_ids contains unknown product {id}"
                )));
            }
        }
    }
    if let Some(weights) = &input.weight_overrides {
        for (id, weight) in weights {
            if !known.iter().any(|known_id| known_id == id) {
                return Err(StoreError::invalid(format!(
                    "weight_overrides contains unknown product {id}"
                )));
            }
            if *weight <= 0 {
                return Err(StoreError::invalid(
                    "weight_overrides values must be greater than 0",
                ));
            }
        }
    }
    Ok(())
}

fn template_name_taken(
    conn: &Connection,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<bool, StoreError> {
    let count: i64 = if let Some(id) = exclude_id {
        conn.query_row(
            "SELECT COUNT(*) FROM templates WHERE name = ?1 AND id != ?2",
            params![name.trim(), id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM templates WHERE name = ?1",
            params![name.trim()],
            |row| row.get(0),
        )?
    };
    Ok(count > 0)
}
