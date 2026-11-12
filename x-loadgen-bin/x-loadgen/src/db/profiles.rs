//! SQLite persistence and validation for reusable Load Profiles (decoupled from
//! specific requests/collections so any profile can stress-test any endpoint).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::error::StoreError;
use super::workspace::{new_id, non_empty_owned, now_ms};
use crate::run_assert::AssertSpec;

pub const MODEL_OPEN_LOOP: &str = "open_loop";
pub const MODEL_CLOSED_LOOP: &str = "closed_loop";
pub const MODE_TOTAL: &str = "total";
pub const MODE_PER_PROCESS: &str = "per_process";

const fn default_concurrency() -> i64 {
    10
}
const fn default_timeout_secs() -> i64 {
    30
}
const fn default_connect_timeout_secs() -> i64 {
    5
}
const fn default_refresh_skew_secs() -> i64 {
    60
}
const fn default_true() -> bool {
    true
}
const fn default_processes() -> i64 {
    1
}
fn default_model() -> String {
    MODEL_OPEN_LOOP.to_string()
}
fn default_split_mode() -> String {
    MODE_TOTAL.to_string()
}

/// Core load-shaping parameters shared between saved [`LoadProfile`] rows,
/// ad-hoc run requests, and frozen [`crate::engine::RunSnapshot`]s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qps: Option<i64>,
    #[serde(default = "default_concurrency")]
    pub concurrency: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<i64>,
    #[serde(default)]
    pub warmup_secs: i64,
    #[serde(default)]
    pub ramp_secs: i64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: i64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: i64,
    #[serde(default = "default_refresh_skew_secs")]
    pub token_refresh_skew_secs: i64,
    #[serde(default = "default_true")]
    pub include_request_id: bool,
    #[serde(default = "default_processes")]
    pub processes: i64,
    #[serde(default = "default_split_mode")]
    pub qps_mode: String,
    #[serde(default = "default_split_mode")]
    pub requests_mode: String,
    #[serde(default)]
    pub loop_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<i64>,
    #[serde(default)]
    pub loop_interval_secs: i64,
    #[serde(default)]
    pub expected_reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_retain_days: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_retain_rows: Option<i64>,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            qps: Some(20),
            concurrency: default_concurrency(),
            requests: None,
            duration_secs: Some(10),
            warmup_secs: 0,
            ramp_secs: 0,
            timeout_secs: default_timeout_secs(),
            connect_timeout_secs: default_connect_timeout_secs(),
            token_refresh_skew_secs: default_refresh_skew_secs(),
            include_request_id: true,
            processes: 1,
            qps_mode: default_split_mode(),
            requests_mode: default_split_mode(),
            loop_enabled: false,
            rounds: None,
            loop_interval_secs: 0,
            expected_reasons: Vec::new(),
            detail_retain_days: None,
            detail_retain_rows: None,
        }
    }
}

impl LoadConfig {
    /// Validate invariants and global server capacity caps (`max_qps`, `max_concurrency`).
    ///
    /// # Errors
    ///
    /// Returns validation error on out-of-range values or invalid combinations.
    pub fn validate(
        &self,
        max_qps: Option<u64>,
        max_concurrency: Option<u64>,
    ) -> Result<(), StoreError> {
        match self.model.trim() {
            MODEL_OPEN_LOOP => {
                let qps = self.qps.ok_or_else(|| {
                    StoreError::invalid("qps is required when model is 'open_loop'")
                })?;
                if qps <= 0 {
                    return Err(StoreError::invalid("qps must be > 0"));
                }
            }
            MODEL_CLOSED_LOOP => {
                if let Some(qps) = self.qps
                    && qps <= 0
                {
                    return Err(StoreError::invalid("qps must be > 0 when set"));
                }
            }
            other => {
                return Err(StoreError::invalid(format!(
                    "model must be 'open_loop' or 'closed_loop', got {other:?}"
                )));
            }
        }
        if self.concurrency < 1 {
            return Err(StoreError::invalid("concurrency must be >= 1"));
        }
        match (self.requests, self.duration_secs) {
            (Some(r), None) => {
                if r < 1 {
                    return Err(StoreError::invalid("requests must be >= 1"));
                }
            }
            (None, Some(d)) => {
                if d < 1 {
                    return Err(StoreError::invalid("duration_secs must be >= 1"));
                }
            }
            (None, None) => {
                return Err(StoreError::invalid(
                    "either requests or duration_secs must be set",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(StoreError::invalid(
                    "requests and duration_secs are mutually exclusive (set one, not both)",
                ));
            }
        }
        if self.warmup_secs < 0 {
            return Err(StoreError::invalid("warmup_secs must be >= 0"));
        }
        if self.ramp_secs < 0 {
            return Err(StoreError::invalid("ramp_secs must be >= 0"));
        }
        if self.timeout_secs < 1 {
            return Err(StoreError::invalid("timeout_secs must be >= 1"));
        }
        if self.connect_timeout_secs < 1 {
            return Err(StoreError::invalid("connect_timeout_secs must be >= 1"));
        }
        if self.token_refresh_skew_secs < 0 {
            return Err(StoreError::invalid("token_refresh_skew_secs must be >= 0"));
        }
        if self.processes < 1 {
            return Err(StoreError::invalid("processes must be >= 1"));
        }
        if !matches!(self.qps_mode.trim(), MODE_TOTAL | MODE_PER_PROCESS) {
            return Err(StoreError::invalid(
                "qps_mode must be 'total' or 'per_process'",
            ));
        }
        if !matches!(self.requests_mode.trim(), MODE_TOTAL | MODE_PER_PROCESS) {
            return Err(StoreError::invalid(
                "requests_mode must be 'total' or 'per_process'",
            ));
        }
        if let Some(rounds) = self.rounds
            && rounds < 1
        {
            return Err(StoreError::invalid("rounds must be >= 1 when set"));
        }
        if self.loop_interval_secs < 0 {
            return Err(StoreError::invalid("loop_interval_secs must be >= 0"));
        }
        self.validate_retention_and_caps(max_qps, max_concurrency)
    }

    fn validate_retention_and_caps(
        &self,
        max_qps: Option<u64>,
        max_concurrency: Option<u64>,
    ) -> Result<(), StoreError> {
        if let Some(days) = self.detail_retain_days
            && days < 1
        {
            return Err(StoreError::invalid("detail_retain_days must be >= 1"));
        }
        if let Some(rows) = self.detail_retain_rows
            && rows < 1
        {
            return Err(StoreError::invalid("detail_retain_rows must be >= 1"));
        }
        let eff_qps = self.effective_total_qps();
        if let (Some(cap), Some(total_qps)) = (max_qps, eff_qps)
            && u64::try_from(total_qps).unwrap_or(u64::MAX) > cap
        {
            return Err(StoreError::invalid(format!(
                "effective qps ({total_qps}) exceeds server max_qps ({cap})"
            )));
        }
        let eff_conc = self.effective_total_concurrency();
        if let Some(cap) = max_concurrency
            && u64::try_from(eff_conc).unwrap_or(u64::MAX) > cap
        {
            return Err(StoreError::invalid(format!(
                "effective concurrency ({eff_conc}) exceeds server max_concurrency ({cap})"
            )));
        }
        Ok(())
    }

    /// Effective total QPS across all processes (`per_process` multiplies by `processes`).
    #[must_use]
    pub fn effective_total_qps(&self) -> Option<i64> {
        let qps = self.qps?;
        if self.qps_mode.trim() == MODE_PER_PROCESS && self.processes > 1 {
            Some(qps.saturating_mul(self.processes))
        } else {
            Some(qps)
        }
    }

    /// Effective total concurrency across all processes.
    #[must_use]
    pub fn effective_total_concurrency(&self) -> i64 {
        if self.qps_mode.trim() == MODE_PER_PROCESS && self.processes > 1 {
            self.concurrency.saturating_mul(self.processes)
        } else {
            self.concurrency
        }
    }
}

/// Saved reusable Load Profile row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadProfile {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub config: LoadConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assert: Option<AssertSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create/update payload for a [`LoadProfile`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadProfileInput {
    pub name: String,
    #[serde(flatten)]
    pub config: LoadConfig,
    #[serde(default)]
    pub assert: Option<AssertSpec>,
    #[serde(default)]
    pub note: Option<String>,
}

/// List all saved load profiles.
///
/// # Errors
///
/// Returns storage error if the query fails.
pub fn list_load_profiles(conn: &Connection) -> Result<Vec<LoadProfile>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, model, qps, concurrency, requests, duration_secs,
                warmup_secs, ramp_secs, timeout_secs, connect_timeout_secs,
                token_refresh_skew_secs, include_request_id, processes,
                qps_mode, requests_mode, loop_enabled, rounds, loop_interval_secs,
                expected_reasons_json, detail_retain_days, detail_retain_rows,
                assert_json, note, created_at, updated_at
         FROM load_profiles ORDER BY created_at DESC, name ASC",
    )?;
    let rows = stmt.query_map([], profile_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Fetch a load profile by ID or unique name.
///
/// # Errors
///
/// Returns not-found if unknown.
pub fn get_load_profile_by_id_or_name(
    conn: &Connection,
    key: &str,
) -> Result<LoadProfile, StoreError> {
    conn.query_row(
        "SELECT id, name, model, qps, concurrency, requests, duration_secs,
                warmup_secs, ramp_secs, timeout_secs, connect_timeout_secs,
                token_refresh_skew_secs, include_request_id, processes,
                qps_mode, requests_mode, loop_enabled, rounds, loop_interval_secs,
                expected_reasons_json, detail_retain_days, detail_retain_rows,
                assert_json, note, created_at, updated_at
         FROM load_profiles WHERE id = ?1 OR name = ?1 LIMIT 1",
        params![key],
        profile_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("load profile {key} not found")))
}

/// Fetch a load profile by ID.
///
/// # Errors
///
/// Returns not-found if unknown.
pub fn get_load_profile(conn: &Connection, id: &str) -> Result<LoadProfile, StoreError> {
    conn.query_row(
        "SELECT id, name, model, qps, concurrency, requests, duration_secs,
                warmup_secs, ramp_secs, timeout_secs, connect_timeout_secs,
                token_refresh_skew_secs, include_request_id, processes,
                qps_mode, requests_mode, loop_enabled, rounds, loop_interval_secs,
                expected_reasons_json, detail_retain_days, detail_retain_rows,
                assert_json, note, created_at, updated_at
         FROM load_profiles WHERE id = ?1",
        params![id],
        profile_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("load profile {id} not found")))
}

/// Create a load profile.
///
/// # Errors
///
/// Returns validation/conflict/storage error.
pub fn create_load_profile(
    conn: &Connection,
    input: &LoadProfileInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<LoadProfile, StoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("load profile name must not be empty"));
    }
    input.config.validate(max_qps, max_concurrency)?;
    if let Some(spec) = input.assert.as_ref() {
        spec.validate()?;
        if spec.min_qps_ratio.is_some() && input.config.qps.is_none() {
            return Err(StoreError::invalid(
                "assert.min_qps_ratio requires qps to be configured",
            ));
        }
    }
    let id = new_id();
    let now = now_ms()?;
    let assert_json = input
        .assert
        .as_ref()
        .and_then(|s| serde_json::to_string(s).ok());
    conn.execute(
        "INSERT INTO load_profiles (
            id, name, model, qps, concurrency, requests, duration_secs,
            warmup_secs, ramp_secs, timeout_secs, connect_timeout_secs,
            token_refresh_skew_secs, include_request_id, processes,
            qps_mode, requests_mode, loop_enabled, rounds, loop_interval_secs,
            expected_reasons_json, detail_retain_days, detail_retain_rows,
            assert_json, note, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11,
            ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19,
            ?20, ?21, ?22,
            ?23, ?24, ?25, ?25
         )",
        params![
            id,
            name,
            input.config.model.trim(),
            input.config.qps,
            input.config.concurrency,
            input.config.requests,
            input.config.duration_secs,
            input.config.warmup_secs,
            input.config.ramp_secs,
            input.config.timeout_secs,
            input.config.connect_timeout_secs,
            input.config.token_refresh_skew_secs,
            i64::from(input.config.include_request_id),
            input.config.processes,
            input.config.qps_mode.trim(),
            input.config.requests_mode.trim(),
            i64::from(input.config.loop_enabled),
            input.config.rounds,
            input.config.loop_interval_secs,
            serde_json::to_string(&input.config.expected_reasons).unwrap_or_else(|_| "[]".into()),
            input.config.detail_retain_days,
            input.config.detail_retain_rows,
            assert_json,
            non_empty_owned(input.note.as_deref()),
            now
        ],
    )
    .map_err(|err| map_unique_err(err, name))?;
    get_load_profile(conn, &id)
}

/// Update an existing load profile.
///
/// # Errors
///
/// Returns validation/not-found/conflict/storage error.
pub fn update_load_profile(
    conn: &Connection,
    id: &str,
    input: &LoadProfileInput,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<LoadProfile, StoreError> {
    let _ = get_load_profile(conn, id)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("load profile name must not be empty"));
    }
    input.config.validate(max_qps, max_concurrency)?;
    if let Some(spec) = input.assert.as_ref() {
        spec.validate()?;
        if spec.min_qps_ratio.is_some() && input.config.qps.is_none() {
            return Err(StoreError::invalid(
                "assert.min_qps_ratio requires qps to be configured",
            ));
        }
    }
    let now = now_ms()?;
    let assert_json = input
        .assert
        .as_ref()
        .and_then(|s| serde_json::to_string(s).ok());
    conn.execute(
        "UPDATE load_profiles SET
            name = ?1, model = ?2, qps = ?3, concurrency = ?4, requests = ?5,
            duration_secs = ?6, warmup_secs = ?7, ramp_secs = ?8, timeout_secs = ?9,
            connect_timeout_secs = ?10, token_refresh_skew_secs = ?11,
            include_request_id = ?12, processes = ?13, qps_mode = ?14,
            requests_mode = ?15, loop_enabled = ?16, rounds = ?17,
            loop_interval_secs = ?18, expected_reasons_json = ?19,
            detail_retain_days = ?20, detail_retain_rows = ?21,
            assert_json = ?22, note = ?23, updated_at = ?24
         WHERE id = ?25",
        params![
            name,
            input.config.model.trim(),
            input.config.qps,
            input.config.concurrency,
            input.config.requests,
            input.config.duration_secs,
            input.config.warmup_secs,
            input.config.ramp_secs,
            input.config.timeout_secs,
            input.config.connect_timeout_secs,
            input.config.token_refresh_skew_secs,
            i64::from(input.config.include_request_id),
            input.config.processes,
            input.config.qps_mode.trim(),
            input.config.requests_mode.trim(),
            i64::from(input.config.loop_enabled),
            input.config.rounds,
            input.config.loop_interval_secs,
            serde_json::to_string(&input.config.expected_reasons).unwrap_or_else(|_| "[]".into()),
            input.config.detail_retain_days,
            input.config.detail_retain_rows,
            assert_json,
            non_empty_owned(input.note.as_deref()),
            now,
            id
        ],
    )
    .map_err(|err| map_unique_err(err, name))?;
    get_load_profile(conn, id)
}

/// Delete a load profile (rejects if an active run currently references it).
///
/// # Errors
///
/// Returns conflict if referenced by an active run, or not-found if missing.
pub fn delete_load_profile(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM runs
         WHERE template_id = ?1 AND status IN ('running', 'stopping', 'interrupted')",
        params![id],
        |row| row.get(0),
    )?;
    if active > 0 {
        return Err(StoreError::conflict(
            "cannot delete load profile while referenced by an active or interrupted run",
        ));
    }
    let changed = conn.execute("DELETE FROM load_profiles WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(StoreError::not_found(format!(
            "load profile {id} not found"
        )));
    }
    Ok(())
}

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LoadProfile> {
    let reasons_raw: String = row.get(19)?;
    let assert_raw: Option<String> = row.get(22)?;
    Ok(LoadProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        config: LoadConfig {
            model: row.get(2)?,
            qps: row.get(3)?,
            concurrency: row.get(4)?,
            requests: row.get(5)?,
            duration_secs: row.get(6)?,
            warmup_secs: row.get(7)?,
            ramp_secs: row.get(8)?,
            timeout_secs: row.get(9)?,
            connect_timeout_secs: row.get(10)?,
            token_refresh_skew_secs: row.get(11)?,
            include_request_id: row.get::<_, i64>(12)? != 0,
            processes: row.get(13)?,
            qps_mode: row.get(14)?,
            requests_mode: row.get(15)?,
            loop_enabled: row.get::<_, i64>(16)? != 0,
            rounds: row.get(17)?,
            loop_interval_secs: row.get(18)?,
            expected_reasons: serde_json::from_str(&reasons_raw).unwrap_or_default(),
            detail_retain_days: row.get(20)?,
            detail_retain_rows: row.get(21)?,
        },
        assert: assert_raw.and_then(|s| serde_json::from_str(&s).ok()),
        note: row.get(23)?,
        created_at: row.get(24)?,
        updated_at: row.get(25)?,
    })
}

fn map_unique_err(err: rusqlite::Error, name: &str) -> StoreError {
    if let rusqlite::Error::SqliteFailure(inner, _) = &err
        && inner.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return StoreError::conflict(format!("load profile '{name}' already exists"));
    }
    StoreError::from(err)
}
