//! SQLite persistence for Hoppscotch-style Environments (including Global
//! variables, default gateway URL, TLS/mTLS, and weighted Mazbri tenant pools).

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::error::StoreError;
use super::workspace::{
    EnvVarRow, GeneratedBindingKeypair, HttpAuthSpec, TenantSpec, TlsSpec, create_binding_keypair,
    new_id, non_empty_owned, now_ms, validate_optional_base_url,
};

/// Reserved deterministic ID for the built-in Global variables environment.
pub const GLOBAL_ENV_ID: &str = "global";

/// Lightweight summary returned by `GET /api/v1/environments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSummary {
    pub id: String,
    pub name: String,
    pub is_global: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_base_url: Option<String>,
    pub variable_count: usize,
    pub tenant_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Full environment detail including variables, TLS, HTTP auth, and weighted tenants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentDetail {
    pub id: String,
    pub name: String,
    pub is_global: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_base_url: Option<String>,
    pub variables: Vec<EnvVarRow>,
    pub http_auth: HttpAuthSpec,
    pub tls: TlsSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub tenants: Vec<TenantSpec>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EnvironmentDetail {
    /// Return a redacted copy safe for non-secret export.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            variables: self.variables.iter().map(EnvVarRow::redacted).collect(),
            http_auth: self.http_auth.redacted(),
            tls: self.tls.redacted(),
            tenants: self.tenants.iter().map(TenantSpec::redacted).collect(),
            ..self.clone()
        }
    }
}

/// Input payload for creating or updating an environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentInput {
    pub name: String,
    #[serde(default)]
    pub gateway_base_url: Option<String>,
    #[serde(default)]
    pub variables: Vec<EnvVarRow>,
    #[serde(default = "HttpAuthSpec::none")]
    pub http_auth: HttpAuthSpec,
    #[serde(default)]
    pub tls: TlsSpec,
    #[serde(default)]
    pub note: Option<String>,
    /// When `Some`, replaces the environment's tenant pool atomically.
    #[serde(default)]
    pub tenants: Option<Vec<TenantSpec>>,
}

/// Ensure the built-in `"Global"` environment row exists.
///
/// # Errors
///
/// Returns a storage error if SQLite cannot insert the row.
pub fn ensure_global_environment(conn: &Connection) -> Result<EnvironmentDetail, StoreError> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT id FROM environments WHERE id = ?1 OR is_global = 1 LIMIT 1",
            params![GLOBAL_ENV_ID],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = exists {
        return get_environment(conn, &id);
    }
    let now = now_ms()?;
    conn.execute(
        "INSERT INTO environments (
            id, name, is_global, gateway_base_url, variables_json,
            http_auth_json, tls_json, note, created_at, updated_at
         ) VALUES (?1, 'Global', 1, NULL, '[]', ?2, '{}', 'Global workspace variables', ?3, ?3)",
        params![
            GLOBAL_ENV_ID,
            serde_json::to_string(&HttpAuthSpec::none()).unwrap_or_else(|_| "{}".into()),
            now
        ],
    )?;
    get_environment(conn, GLOBAL_ENV_ID)
}

/// List all environments (`Global` first, then alphabetical/chronological).
///
/// # Errors
///
/// Returns a storage error if queries fail.
pub fn list_environments(conn: &Connection) -> Result<Vec<EnvironmentSummary>, StoreError> {
    let _ = ensure_global_environment(conn)?;
    let mut stmt = conn.prepare(
        "SELECT e.id, e.name, e.is_global, e.gateway_base_url, e.variables_json,
                e.note, e.created_at, e.updated_at,
                (SELECT COUNT(*) FROM environment_tenants t WHERE t.environment_id = e.id)
         FROM environments e
         ORDER BY e.is_global DESC, e.created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let vars_raw: String = row.get(4)?;
        let vars: Vec<EnvVarRow> = serde_json::from_str(&vars_raw).unwrap_or_default();
        let tenant_count: i64 = row.get(8)?;
        Ok(EnvironmentSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            is_global: row.get::<_, i64>(2)? != 0,
            gateway_base_url: row.get(3)?,
            variable_count: vars.len(),
            tenant_count: usize::try_from(tenant_count.max(0)).unwrap_or(0),
            note: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Create a named environment (with optional initial tenant pool).
///
/// # Errors
///
/// Returns validation/conflict/storage error.
pub fn create_environment(
    conn: &Connection,
    input: &EnvironmentInput,
) -> Result<EnvironmentDetail, StoreError> {
    let _ = ensure_global_environment(conn)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("environment name must not be empty"));
    }
    if name.eq_ignore_ascii_case("Global") {
        return Err(StoreError::conflict(
            "environment name 'Global' is reserved for global variables",
        ));
    }
    let gateway_base_url =
        validate_optional_base_url(input.gateway_base_url.as_deref(), "gateway_base_url")?;
    input.http_auth.validate()?;
    input.tls.validate()?;
    if let Some(tenants) = input.tenants.as_ref() {
        validate_tenant_slice(tenants)?;
    }
    let id = new_id();
    let now = now_ms()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO environments (
            id, name, is_global, gateway_base_url, variables_json,
            http_auth_json, tls_json, note, created_at, updated_at
         ) VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            name,
            gateway_base_url,
            serde_json::to_string(&input.variables).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&input.http_auth).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&input.tls).unwrap_or_else(|_| "{}".into()),
            non_empty_owned(input.note.as_deref()),
            now
        ],
    )
    .map_err(|err| map_unique_err(err, name))?;
    if let Some(tenants) = input.tenants.as_ref() {
        replace_tenants(&tx, &id, tenants, &[])?;
    }
    tx.commit()?;
    get_environment(conn, &id)
}

/// Update an environment (including `"global"` variables if `id == "global"`).
///
/// # Errors
///
/// Returns validation/not-found/conflict/storage error.
pub fn update_environment(
    conn: &Connection,
    id: &str,
    input: &EnvironmentInput,
) -> Result<EnvironmentDetail, StoreError> {
    let _ = ensure_global_environment(conn)?;
    let existing = get_environment(conn, id)?;
    let name = if existing.is_global {
        "Global".to_string()
    } else {
        let trimmed = input.name.trim();
        if trimmed.is_empty() {
            return Err(StoreError::invalid("environment name must not be empty"));
        }
        if trimmed.eq_ignore_ascii_case("Global") {
            return Err(StoreError::conflict(
                "environment name 'Global' is reserved for global variables",
            ));
        }
        trimmed.to_string()
    };
    let gateway_base_url =
        validate_optional_base_url(input.gateway_base_url.as_deref(), "gateway_base_url")?;
    input.http_auth.validate()?;
    input.tls.validate()?;
    if let Some(tenants) = input.tenants.as_ref() {
        validate_tenant_slice(tenants)?;
    }
    let now = now_ms()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE environments SET
            name = ?1, gateway_base_url = ?2, variables_json = ?3,
            http_auth_json = ?4, tls_json = ?5, note = ?6, updated_at = ?7
         WHERE id = ?8",
        params![
            name,
            gateway_base_url,
            serde_json::to_string(&input.variables).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&input.http_auth).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&input.tls).unwrap_or_else(|_| "{}".into()),
            non_empty_owned(input.note.as_deref()),
            now,
            id
        ],
    )
    .map_err(|err| map_unique_err(err, &name))?;
    if let Some(tenants) = input.tenants.as_ref() {
        replace_tenants(&tx, id, tenants, &existing.tenants)?;
    }
    tx.commit()?;
    get_environment(conn, id)
}

/// Delete a non-global environment.
///
/// # Errors
///
/// Returns validation error if attempting to delete `Global`, or not-found if missing.
pub fn delete_environment(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let existing = get_environment(conn, id)?;
    if existing.is_global {
        return Err(StoreError::invalid(
            "the built-in Global environment cannot be deleted",
        ));
    }
    conn.execute("DELETE FROM environments WHERE id = ?1", params![id])?;
    Ok(())
}

/// Fetch one environment and its ordered tenant pool.
///
/// # Errors
///
/// Returns not-found if `id` is unknown.
pub fn get_environment(conn: &Connection, id: &str) -> Result<EnvironmentDetail, StoreError> {
    if id == GLOBAL_ENV_ID {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM environments WHERE id = ?1",
            params![GLOBAL_ENV_ID],
            |row| row.get(0),
        )?;
        if count == 0 {
            return ensure_global_environment(conn);
        }
    }
    let mut detail = conn
        .query_row(
            "SELECT id, name, is_global, gateway_base_url, variables_json,
                    http_auth_json, tls_json, note, created_at, updated_at
             FROM environments WHERE id = ?1",
            params![id],
            |row| {
                let vars_raw: String = row.get(4)?;
                let auth_raw: String = row.get(5)?;
                let tls_raw: String = row.get(6)?;
                Ok(EnvironmentDetail {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    is_global: row.get::<_, i64>(2)? != 0,
                    gateway_base_url: row.get(3)?,
                    variables: serde_json::from_str(&vars_raw).unwrap_or_default(),
                    http_auth: serde_json::from_str(&auth_raw)
                        .unwrap_or_else(|_| HttpAuthSpec::none()),
                    tls: serde_json::from_str(&tls_raw).unwrap_or_default(),
                    note: row.get(7)?,
                    tenants: Vec::new(),
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::not_found(format!("environment {id} not found")))?;
    detail.tenants = list_tenants(conn, id)?;
    Ok(detail)
}

/// List all tenants in an environment's pool.
///
/// # Errors
///
/// Returns not-found if `environment_id` does not exist.
pub fn list_tenants(
    conn: &Connection,
    environment_id: &str,
) -> Result<Vec<TenantSpec>, StoreError> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT id FROM environments WHERE id = ?1",
            params![environment_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StoreError::not_found(format!(
            "environment {environment_id} not found"
        )));
    }
    let mut stmt = conn.prepare(
        "SELECT id, name, weight, api_key, encryption_mode, tenant_id, tenant_key_base64,
                key_version, suite, data_alg, mac_alg, require_signature, allow_insecure,
                binding_kid, binding_alg, binding_key, binding_public_key
         FROM environment_tenants
         WHERE environment_id = ?1
         ORDER BY created_at ASC, name ASC",
    )?;
    let rows = stmt.query_map(params![environment_id], tenant_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Fetch one tenant from an environment.
///
/// # Errors
///
/// Returns not-found if either `environment_id` or `tenant_id` is unknown.
pub fn get_tenant(
    conn: &Connection,
    environment_id: &str,
    tenant_id: &str,
) -> Result<TenantSpec, StoreError> {
    conn.query_row(
        "SELECT id, name, weight, api_key, encryption_mode, tenant_id, tenant_key_base64,
                key_version, suite, data_alg, mac_alg, require_signature, allow_insecure,
                binding_kid, binding_alg, binding_key, binding_public_key
         FROM environment_tenants
         WHERE environment_id = ?1 AND id = ?2",
        params![environment_id, tenant_id],
        tenant_from_row,
    )
    .optional()?
    .ok_or_else(|| {
        StoreError::not_found(format!(
            "tenant {tenant_id} not found in environment {environment_id}"
        ))
    })
}

/// Add one tenant to an environment pool.
///
/// # Errors
///
/// Returns validation/not-found/conflict/storage error.
pub fn create_tenant(
    conn: &Connection,
    environment_id: &str,
    spec: &TenantSpec,
) -> Result<TenantSpec, StoreError> {
    let _ = get_environment(conn, environment_id)?;
    let mut cleaned = spec.clone();
    normalize_tenant_public_key(&mut cleaned, None);
    cleaned.validate_pool_entry()?;
    let id = new_id();
    let now = now_ms()?;
    insert_tenant_row(conn, environment_id, &id, &cleaned, now)
        .map_err(|err| map_unique_err(err, cleaned.name.trim()))?;
    get_tenant(conn, environment_id, &id)
}

/// Update one tenant in an environment pool (preserving `binding_public_key` if key unchanged).
///
/// # Errors
///
/// Returns validation/not-found/conflict/storage error.
pub fn update_tenant(
    conn: &Connection,
    environment_id: &str,
    tenant_row_id: &str,
    spec: &TenantSpec,
) -> Result<TenantSpec, StoreError> {
    let existing = get_tenant(conn, environment_id, tenant_row_id)?;
    let mut merged = spec.clone();
    normalize_tenant_public_key(&mut merged, Some(&existing));
    merged.validate_pool_entry()?;
    let now = now_ms()?;
    conn.execute(
        "UPDATE environment_tenants SET
            name = ?1, weight = ?2, api_key = ?3, encryption_mode = ?4,
            tenant_id = ?5, tenant_key_base64 = ?6, key_version = ?7,
            suite = ?8, data_alg = ?9, mac_alg = ?10, require_signature = ?11,
            allow_insecure = ?12, binding_kid = ?13, binding_alg = ?14,
            binding_key = ?15, binding_public_key = ?16, updated_at = ?17
         WHERE environment_id = ?18 AND id = ?19",
        params![
            merged.name.trim(),
            merged.weight,
            non_empty_owned(merged.api_key.as_deref()),
            merged.encryption_mode.trim(),
            non_empty_owned(merged.tenant_id.as_deref()),
            non_empty_owned(merged.tenant_key_base64.as_deref()),
            non_empty_owned(merged.key_version.as_deref()),
            non_empty_owned(merged.suite.as_deref()),
            non_empty_owned(merged.data_alg.as_deref()),
            non_empty_owned(merged.mac_alg.as_deref()),
            merged.require_signature.map(i64::from),
            i64::from(merged.allow_insecure),
            non_empty_owned(merged.binding_kid.as_deref()),
            non_empty_owned(merged.binding_alg.as_deref()),
            non_empty_owned(merged.binding_key.as_deref()),
            non_empty_owned(merged.binding_public_key.as_deref()),
            now,
            environment_id,
            tenant_row_id
        ],
    )
    .map_err(|err| map_unique_err(err, merged.name.trim()))?;
    get_tenant(conn, environment_id, tenant_row_id)
}

/// Delete one tenant from an environment pool.
///
/// # Errors
///
/// Returns not-found if missing.
pub fn delete_tenant(
    conn: &Connection,
    environment_id: &str,
    tenant_row_id: &str,
) -> Result<(), StoreError> {
    let changed = conn.execute(
        "DELETE FROM environment_tenants WHERE environment_id = ?1 AND id = ?2",
        params![environment_id, tenant_row_id],
    )?;
    if changed == 0 {
        return Err(StoreError::not_found(format!(
            "tenant {tenant_row_id} not found in environment {environment_id}"
        )));
    }
    Ok(())
}

/// Generate an asymmetric request-binding keypair for a saved environment tenant,
/// storing the private PKCS#8 PEM in `binding_key` and SPKI PEM in `binding_public_key`.
///
/// # Errors
///
/// Returns validation/not-found/storage error.
pub fn generate_tenant_binding_keypair(
    conn: &Connection,
    environment_id: &str,
    tenant_row_id: &str,
    alg: &str,
    kid_override: Option<&str>,
) -> Result<GeneratedBindingKeypair, StoreError> {
    let existing = get_tenant(conn, environment_id, tenant_row_id)?;
    let effective_kid = non_empty_owned(kid_override)
        .or_else(|| non_empty_owned(existing.binding_kid.as_deref()))
        .ok_or_else(|| {
            StoreError::invalid(
                "binding_kid is required (provide `kid` in request or save binding_kid on tenant first)",
            )
        })?;
    let (private_pem, out) = create_binding_keypair(alg, &effective_kid, false)?;
    let now = now_ms()?;
    conn.execute(
        "UPDATE environment_tenants SET
            binding_kid = ?1, binding_alg = ?2, binding_key = ?3,
            binding_public_key = ?4, updated_at = ?5
         WHERE environment_id = ?6 AND id = ?7",
        params![
            out.binding_kid,
            out.binding_alg,
            private_pem,
            out.public_key_pem,
            now,
            environment_id,
            tenant_row_id
        ],
    )?;
    Ok(out)
}

/// Build the merged static variable map from `Global` + optional selected `Environment`.
/// Automatically injects `baseUrl` and `gateway_base_url` when `env.gateway_base_url`
/// is set and not explicitly overridden in `variables`.
///
/// # Errors
///
/// Returns not-found/storage error if `environment_id` is specified and missing.
pub fn resolve_environment_context(
    conn: &Connection,
    environment_id: Option<&str>,
) -> Result<(Option<EnvironmentDetail>, BTreeMap<String, String>), StoreError> {
    let global = ensure_global_environment(conn)?;
    let mut vars = BTreeMap::new();
    for v in &global.variables {
        if v.active && !v.key.trim().is_empty() {
            vars.insert(v.key.trim().to_string(), v.value.clone());
        }
    }
    let selected = match non_empty_owned(environment_id) {
        Some(id) if id != GLOBAL_ENV_ID => Some(get_environment(conn, &id)?),
        _ => None,
    };
    if let Some(env) = selected.as_ref() {
        if let Some(base) = env.gateway_base_url.as_deref()
            && !base.trim().is_empty()
        {
            vars.entry("baseUrl".to_string())
                .or_insert_with(|| base.trim().to_string());
            vars.entry("gateway_base_url".to_string())
                .or_insert_with(|| base.trim().to_string());
        }
        for v in &env.variables {
            if v.active && !v.key.trim().is_empty() {
                vars.insert(v.key.trim().to_string(), v.value.clone());
            }
        }
    }
    Ok((selected, vars))
}

/// Export environments to JSON (redacted by default unless `include_secrets` is true).
///
/// # Errors
///
/// Returns not-found/storage error.
pub fn export_environments_json(
    conn: &Connection,
    environment_id: Option<&str>,
    include_secrets: bool,
) -> Result<Value, StoreError> {
    let _ = ensure_global_environment(conn)?;
    let list = if let Some(id) = environment_id {
        vec![get_environment(conn, id)?]
    } else {
        let summaries = list_environments(conn)?;
        let mut full = Vec::with_capacity(summaries.len());
        for s in summaries {
            full.push(get_environment(conn, &s.id)?);
        }
        full
    };
    let out: Vec<EnvironmentDetail> = if include_secrets {
        list
    } else {
        list.iter().map(EnvironmentDetail::redacted).collect()
    };
    Ok(json!({
        "format": "x-loadgen-environments",
        "version": 2,
        "environments": out,
    }))
}

/// Import Hoppscotch Environment JSON (`{ name, variables: [...] }` or array) or native bundle.
///
/// # Errors
///
/// Returns validation/conflict/storage error.
pub fn import_environments_json(
    conn: &Connection,
    payload: &Value,
) -> Result<Vec<EnvironmentDetail>, StoreError> {
    let items: Vec<&Value> = if let Some(arr) = payload.as_array() {
        arr.iter().collect()
    } else if let Some(arr) = payload.get("environments").and_then(Value::as_array) {
        arr.iter().collect()
    } else if payload.is_object() && payload.get("name").is_some() {
        vec![payload]
    } else {
        return Err(StoreError::invalid(
            "expected a Hoppscotch environment object/array or x-loadgen environments bundle",
        ));
    };
    let mut created = Vec::new();
    for item in items {
        let is_global = item
            .get("is_global")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || item
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.trim().eq_ignore_ascii_case("Global"));
        let variables: Vec<EnvVarRow> = item
            .get("variables")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let gateway_base_url = item
            .get("gateway_base_url")
            .and_then(Value::as_str)
            .map(str::to_string);
        let http_auth: HttpAuthSpec = item
            .get("http_auth")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_else(HttpAuthSpec::none);
        let tls: TlsSpec = item
            .get("tls")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let note = item.get("note").and_then(Value::as_str).map(str::to_string);
        let tenants: Option<Vec<TenantSpec>> = item
            .get("tenants")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let input = EnvironmentInput {
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Imported Environment")
                .to_string(),
            gateway_base_url,
            variables,
            http_auth,
            tls,
            note,
            tenants,
        };
        if is_global {
            created.push(update_environment(conn, GLOBAL_ENV_ID, &input)?);
        } else {
            created.push(create_environment(conn, &input)?);
        }
    }
    Ok(created)
}

fn validate_tenant_slice(tenants: &[TenantSpec]) -> Result<(), StoreError> {
    let mut names = std::collections::HashSet::new();
    for tenant in tenants {
        tenant.validate_pool_entry()?;
        let key = tenant.name.trim().to_string();
        if !names.insert(key.clone()) {
            return Err(StoreError::invalid(format!(
                "duplicate tenant name '{key}' in environment"
            )));
        }
    }
    Ok(())
}

fn replace_tenants(
    conn: &Connection,
    environment_id: &str,
    incoming: &[TenantSpec],
    previous: &[TenantSpec],
) -> Result<(), StoreError> {
    let prev_by_name: HashMap<&str, &TenantSpec> =
        previous.iter().map(|t| (t.name.trim(), t)).collect();
    conn.execute(
        "DELETE FROM environment_tenants WHERE environment_id = ?1",
        params![environment_id],
    )?;
    let now = now_ms()?;
    for item in incoming {
        let mut merged = item.clone();
        let row_id = if let Some(old) = prev_by_name.get(item.name.trim()) {
            normalize_tenant_public_key(&mut merged, Some(old));
            old.id.clone()
        } else {
            normalize_tenant_public_key(&mut merged, None);
            if merged.id.trim().is_empty() {
                new_id()
            } else {
                merged.id.clone()
            }
        };
        insert_tenant_row(conn, environment_id, &row_id, &merged, now)?;
    }
    Ok(())
}

fn normalize_tenant_public_key(merged: &mut TenantSpec, existing: Option<&TenantSpec>) {
    let is_asym = merged
        .binding_alg
        .as_deref()
        .map(str::trim)
        .is_some_and(|alg| !alg.is_empty() && !alg.to_ascii_lowercase().starts_with("hmac-"));
    if !is_asym {
        merged.binding_public_key = None;
        return;
    }
    if let Some(old) = existing {
        if old.binding_alg == merged.binding_alg && old.binding_key == merged.binding_key {
            if merged.binding_public_key.is_none() {
                merged
                    .binding_public_key
                    .clone_from(&old.binding_public_key);
            }
        } else if merged.binding_public_key == old.binding_public_key {
            merged.binding_public_key = None;
        }
    }
}

fn insert_tenant_row(
    conn: &Connection,
    environment_id: &str,
    row_id: &str,
    spec: &TenantSpec,
    now: i64,
) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "INSERT INTO environment_tenants (
            id, environment_id, name, weight, api_key, encryption_mode,
            tenant_id, tenant_key_base64, key_version, suite, data_alg, mac_alg,
            require_signature, allow_insecure, binding_kid, binding_alg,
            binding_key, binding_public_key, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
        params![
            row_id,
            environment_id,
            spec.name.trim(),
            spec.weight,
            non_empty_owned(spec.api_key.as_deref()),
            spec.encryption_mode.trim(),
            non_empty_owned(spec.tenant_id.as_deref()),
            non_empty_owned(spec.tenant_key_base64.as_deref()),
            non_empty_owned(spec.key_version.as_deref()),
            non_empty_owned(spec.suite.as_deref()),
            non_empty_owned(spec.data_alg.as_deref()),
            non_empty_owned(spec.mac_alg.as_deref()),
            spec.require_signature.map(i64::from),
            i64::from(spec.allow_insecure),
            non_empty_owned(spec.binding_kid.as_deref()),
            non_empty_owned(spec.binding_alg.as_deref()),
            non_empty_owned(spec.binding_key.as_deref()),
            non_empty_owned(spec.binding_public_key.as_deref()),
            now
        ],
    )
}

fn tenant_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TenantSpec> {
    let req_sig: Option<i64> = row.get(11)?;
    Ok(TenantSpec {
        id: row.get(0)?,
        name: row.get(1)?,
        weight: row.get(2)?,
        api_key: row.get(3)?,
        encryption_mode: row.get(4)?,
        tenant_id: row.get(5)?,
        tenant_key_base64: row.get(6)?,
        key_version: row.get(7)?,
        suite: row.get(8)?,
        data_alg: row.get(9)?,
        mac_alg: row.get(10)?,
        require_signature: req_sig.map(|v| v != 0),
        allow_insecure: row.get::<_, i64>(12)? != 0,
        binding_kid: row.get(13)?,
        binding_alg: row.get(14)?,
        binding_key: row.get(15)?,
        binding_public_key: row.get(16)?,
    })
}

fn map_unique_err(err: rusqlite::Error, name: &str) -> StoreError {
    if let rusqlite::Error::SqliteFailure(inner, _) = &err
        && inner.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return StoreError::conflict(format!("'{name}' already exists"));
    }
    StoreError::from(err)
}
