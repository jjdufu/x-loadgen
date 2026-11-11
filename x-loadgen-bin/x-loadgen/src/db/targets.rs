//! Persistence for targets, tenants, and products.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use x_loadgen_sdk::{TenantSettings, generate_binding_keypair};

use super::error::StoreError;
use super::unix_now_ms;

/// Generic HTTP target: the product path is called directly on `gateway_base_url`.
pub const PROTOCOL_HTTP: &str = "http";
/// mazbri gateway: token exchange, `POST /api/v1/gateway`, optional binding and encryption.
pub const PROTOCOL_MAZBRI: &str = "mazbri";

/// Target fields accepted by create and full update.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetInput {
    pub name: String,
    /// `http` or `mazbri`.
    #[serde(default = "default_protocol")]
    pub protocol: String,
    pub gateway_base_url: String,
    #[serde(default)]
    pub client_cert: Option<String>,
    #[serde(default)]
    pub client_key: Option<String>,
    #[serde(default)]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub insecure_skip_verify: bool,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub tenants: Option<Vec<TenantInput>>,
    #[serde(default)]
    pub products: Option<Vec<ProductInput>>,
}

/// Tenant fields accepted by create and update.
#[derive(Debug, Clone, Deserialize)]
pub struct TenantInput {
    pub name: String,
    #[serde(default = "default_weight")]
    pub weight: i64,
    /// Required for `mazbri`; optional (`X-API-KEY` header) for `http`.
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_encryption_mode")]
    pub encryption_mode: String,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub tenant_key_base64: Option<String>,
    #[serde(default)]
    pub key_version: Option<String>,
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub data_alg: Option<String>,
    #[serde(default)]
    pub mac_alg: Option<String>,
    #[serde(default)]
    pub require_signature: Option<bool>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default)]
    pub binding_kid: Option<String>,
    #[serde(default)]
    pub binding_alg: Option<String>,
    #[serde(default)]
    pub binding_key: Option<String>,
}

/// Product fields accepted by create and update.
#[derive(Debug, Clone, Deserialize)]
pub struct ProductInput {
    pub name: String,
    pub product_id: String,
    #[serde(default)]
    pub channel_code: Option<String>,
    pub path: String,
    pub method: String,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default)]
    pub qps_limit: Option<i64>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub query_json: Option<Value>,
    #[serde(default)]
    pub headers_json: Option<Value>,
    #[serde(default)]
    pub body_json: Option<Value>,
    #[serde(default)]
    pub body_image_dir: Option<String>,
    #[serde(default)]
    pub body_image_field: Option<String>,
    #[serde(default)]
    pub body_image_name_field: Option<String>,
    #[serde(default)]
    pub expect_transport: Option<String>,
    #[serde(default)]
    pub expect_checks_json: Option<Value>,
}

fn default_weight() -> i64 {
    1
}

fn default_protocol() -> String {
    PROTOCOL_HTTP.to_string()
}

fn default_encryption_mode() -> String {
    "none".to_string()
}

/// Target row used in list responses (no key material).
#[derive(Debug, Clone, Serialize)]
pub struct TargetSummary {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub gateway_base_url: String,
    pub insecure_skip_verify: bool,
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Target detail including plaintext credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetDetail {
    pub id: String,
    pub name: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    pub gateway_base_url: String,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub ca_cert: Option<String>,
    pub insecure_skip_verify: bool,
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub tenants: Vec<Tenant>,
    pub products: Vec<Product>,
}

/// A tenant attached to a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub target_id: String,
    pub name: String,
    pub weight: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub encryption_mode: String,
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_key_base64: Option<String>,
    pub key_version: Option<String>,
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: Option<bool>,
    pub allow_insecure: bool,
    pub binding_kid: Option<String>,
    pub binding_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_key: Option<String>,
    /// Public half of a key pair generated by `binding-keypair` (not secret).
    #[serde(default)]
    pub binding_public_key: Option<String>,
}

impl Tenant {
    /// Drop key material so the value is safe to return in a list.
    #[must_use]
    pub fn redacted(mut self) -> Self {
        self.api_key = None;
        self.tenant_key_base64 = None;
        self.binding_key = None;
        self
    }
}

/// A product attached to a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub target_id: String,
    pub name: String,
    pub product_id: String,
    pub channel_code: Option<String>,
    pub path: String,
    pub method: String,
    pub weight: i64,
    pub qps_limit: Option<i64>,
    pub timeout_secs: Option<i64>,
    pub query_json: Option<Value>,
    pub headers_json: Option<Value>,
    pub body_json: Option<Value>,
    pub body_image_dir: Option<String>,
    pub body_image_field: Option<String>,
    pub body_image_name_field: Option<String>,
    pub expect_transport: Option<String>,
    pub expect_checks_json: Option<Value>,
}

/// List targets without key material.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn list_targets(conn: &Connection) -> Result<Vec<TargetSummary>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, protocol, gateway_base_url, insecure_skip_verify, note,
                created_at, updated_at
         FROM targets ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TargetSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            protocol: row.get(2)?,
            gateway_base_url: row.get(3)?,
            insecure_skip_verify: row.get::<_, i64>(4)? != 0,
            note: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    collect_rows(rows)
}

/// Load a target and its tenants and products (credentials included).
///
/// # Errors
///
/// Returns [`StoreError::NotFound`] if the id is unknown, or a storage error.
pub fn get_target(conn: &Connection, id: &str) -> Result<TargetDetail, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, protocol, gateway_base_url, client_cert, client_key, ca_cert,
                insecure_skip_verify, note, created_at, updated_at
         FROM targets WHERE id = ?1",
    )?;
    let target = stmt
        .query_row(params![id], |row| {
            Ok(TargetDetail {
                id: row.get(0)?,
                name: row.get(1)?,
                protocol: row.get(2)?,
                gateway_base_url: row.get(3)?,
                client_cert: row.get(4)?,
                client_key: row.get(5)?,
                ca_cert: row.get(6)?,
                insecure_skip_verify: row.get::<_, i64>(7)? != 0,
                note: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                tenants: Vec::new(),
                products: Vec::new(),
            })
        })
        .optional()?
        .ok_or_else(|| StoreError::not_found(format!("target {id} not found")))?;
    let mut target = target;
    target.tenants = list_tenants(conn, id, true)?;
    target.products = list_products(conn, id)?;
    Ok(target)
}

/// Insert a target and optional nested tenants and products.
///
/// # Errors
///
/// Returns a validation, conflict, or storage error.
pub fn create_target(
    conn: &mut Connection,
    input: &TargetInput,
) -> Result<TargetDetail, StoreError> {
    validate_target(input)?;
    if name_taken(conn, "targets", &input.name, None)? {
        return Err(StoreError::conflict(format!(
            "a target named '{}' already exists",
            input.name
        )));
    }
    let id = new_id();
    let now = now_ms()?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO targets (
            id, name, protocol, gateway_base_url, client_cert, client_key, ca_cert,
            insecure_skip_verify, note, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            id,
            input.name.trim(),
            input.protocol.trim(),
            input.gateway_base_url.trim(),
            opt_str(input.client_cert.as_deref()),
            opt_str(input.client_key.as_deref()),
            opt_str(input.ca_cert.as_deref()),
            i64::from(input.insecure_skip_verify),
            opt_str(input.note.as_deref()),
            now,
        ],
    )?;
    insert_nested(
        &tx,
        &id,
        input.tenants.as_deref(),
        input.products.as_deref(),
        &[],
    )?;
    tx.commit()?;
    get_target(conn, &id)
}

/// Replace a target's fields; optional tenant/product arrays replace those collections.
///
/// # Errors
///
/// Returns a validation, not-found, conflict, or storage error.
pub fn update_target(
    conn: &mut Connection,
    id: &str,
    input: &TargetInput,
) -> Result<TargetDetail, StoreError> {
    validate_target(input)?;
    let current = get_target(conn, id)?;
    if input.tenants.is_none() {
        let protocol = input.protocol.trim();
        for tenant in &current.tenants {
            validate_tenant(&tenant_as_input(tenant), protocol)?;
        }
    }
    if name_taken(conn, "targets", &input.name, Some(id))? {
        return Err(StoreError::conflict(format!(
            "a target named '{}' already exists",
            input.name
        )));
    }
    let now = now_ms()?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE targets SET name = ?1, protocol = ?2, gateway_base_url = ?3,
            client_cert = ?4, client_key = ?5, ca_cert = ?6, insecure_skip_verify = ?7,
            note = ?8, updated_at = ?9
         WHERE id = ?10",
        params![
            input.name.trim(),
            input.protocol.trim(),
            input.gateway_base_url.trim(),
            opt_str(input.client_cert.as_deref()),
            opt_str(input.client_key.as_deref()),
            opt_str(input.ca_cert.as_deref()),
            i64::from(input.insecure_skip_verify),
            opt_str(input.note.as_deref()),
            now,
            id,
        ],
    )?;
    if input.tenants.is_some() {
        tx.execute(
            "DELETE FROM target_tenants WHERE target_id = ?1",
            params![id],
        )?;
    }
    if input.products.is_some() {
        tx.execute(
            "DELETE FROM target_products WHERE target_id = ?1",
            params![id],
        )?;
    }
    insert_nested(
        &tx,
        id,
        input.tenants.as_deref(),
        input.products.as_deref(),
        &current.tenants,
    )?;
    tx.commit()?;
    get_target(conn, id)
}

/// Delete a target unless templates or unfinished runs still reference it.
///
/// # Errors
///
/// Returns not-found, conflict, or storage errors.
pub fn delete_target(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let _ = get_target(conn, id)?;
    let templates: i64 = conn.query_row(
        "SELECT COUNT(*) FROM templates WHERE target_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if templates > 0 {
        return Err(StoreError::conflict(
            "the target is still referenced by one or more templates",
        ));
    }
    if unfinished_run_count(conn, "target_id", id)? > 0 {
        return Err(StoreError::conflict(
            "the target is still referenced by an unfinished run",
        ));
    }
    conn.execute("DELETE FROM targets WHERE id = ?1", params![id])?;
    Ok(())
}

/// List tenants of a target.
///
/// # Errors
///
/// Returns not-found when the target is missing, or a storage error.
pub fn list_tenants(
    conn: &Connection,
    target_id: &str,
    with_secrets: bool,
) -> Result<Vec<Tenant>, StoreError> {
    ensure_target(conn, target_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, target_id, name, weight, api_key, encryption_mode, tenant_id,
                tenant_key_base64, key_version, suite, data_alg, mac_alg,
                require_signature, allow_insecure, binding_kid, binding_alg, binding_key,
                binding_public_key
         FROM target_tenants WHERE target_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map(params![target_id], |row| {
        Ok(Tenant {
            id: row.get(0)?,
            target_id: row.get(1)?,
            name: row.get(2)?,
            weight: row.get(3)?,
            api_key: row.get(4)?,
            encryption_mode: row.get(5)?,
            tenant_id: row.get(6)?,
            tenant_key_base64: row.get(7)?,
            key_version: row.get(8)?,
            suite: row.get(9)?,
            data_alg: row.get(10)?,
            mac_alg: row.get(11)?,
            require_signature: opt_bool_from_i64(row.get(12)?),
            allow_insecure: row.get::<_, i64>(13)? != 0,
            binding_kid: row.get(14)?,
            binding_alg: row.get(15)?,
            binding_key: row.get(16)?,
            binding_public_key: row.get(17)?,
        })
    })?;
    let mut tenants = collect_rows(rows)?;
    if !with_secrets {
        tenants = tenants.into_iter().map(Tenant::redacted).collect();
    }
    Ok(tenants)
}

/// Load one tenant with credentials.
///
/// # Errors
///
/// Returns not-found when the target or tenant is missing.
pub fn get_tenant(
    conn: &Connection,
    target_id: &str,
    tenant_id: &str,
) -> Result<Tenant, StoreError> {
    let tenants = list_tenants(conn, target_id, true)?;
    tenants
        .into_iter()
        .find(|tenant| tenant.id == tenant_id)
        .ok_or_else(|| StoreError::not_found(format!("tenant {tenant_id} not found")))
}

/// Insert a tenant under a target.
///
/// # Errors
///
/// Returns validation, conflict, or storage errors.
pub fn create_tenant(
    conn: &Connection,
    target_id: &str,
    input: &TenantInput,
) -> Result<Tenant, StoreError> {
    let protocol = target_protocol(conn, target_id)?;
    validate_tenant(input, &protocol)?;
    if tenant_name_taken(conn, target_id, &input.name, None)? {
        return Err(StoreError::conflict(format!(
            "a tenant named '{}' already exists on this target",
            input.name
        )));
    }
    let id = new_id();
    insert_tenant_row(conn, &id, target_id, input, None)?;
    get_tenant(conn, target_id, &id)
}

/// Replace a tenant.
///
/// # Errors
///
/// Returns validation, not-found, conflict, or storage errors.
pub fn update_tenant(
    conn: &Connection,
    target_id: &str,
    tenant_id: &str,
    input: &TenantInput,
) -> Result<Tenant, StoreError> {
    let current = get_tenant(conn, target_id, tenant_id)?;
    let protocol = target_protocol(conn, target_id)?;
    validate_tenant(input, &protocol)?;
    if tenant_name_taken(conn, target_id, &input.name, Some(tenant_id))? {
        return Err(StoreError::conflict(format!(
            "a tenant named '{}' already exists on this target",
            input.name
        )));
    }
    conn.execute(
        "DELETE FROM target_tenants WHERE id = ?1",
        params![tenant_id],
    )?;
    insert_tenant_row(
        conn,
        tenant_id,
        target_id,
        input,
        kept_public_key(&current, input),
    )?;
    get_tenant(conn, target_id, tenant_id)
}

/// Generate a request-binding key pair for a `mazbri` tenant.
///
/// The private key (PKCS#8 PEM) is stored in the tenant row as `binding_key`
/// together with `binding_alg` and the public key; `kid` replaces the tenant's
/// `binding_kid` when given. Returns the updated tenant (the caller decides
/// what to expose).
///
/// # Errors
///
/// Returns not-found, or a validation error when the target is not `mazbri`,
/// the algorithm has no key pair (HMAC / unknown), or no `kid` is available.
pub fn generate_tenant_keypair(
    conn: &Connection,
    target_id: &str,
    tenant_id: &str,
    alg: &str,
    kid: Option<&str>,
) -> Result<Tenant, StoreError> {
    let current = get_tenant(conn, target_id, tenant_id)?;
    let protocol = target_protocol(conn, target_id)?;
    if protocol != PROTOCOL_MAZBRI {
        return Err(StoreError::invalid(
            "request binding key pairs are only used by mazbri targets",
        ));
    }
    let kid = opt_str(kid)
        .or_else(|| opt_str(current.binding_kid.as_deref()))
        .ok_or_else(|| StoreError::invalid("binding_kid is required to generate a key pair"))?
        .to_string();
    let (private_pem, public_pem) =
        generate_binding_keypair(alg).map_err(|error| StoreError::invalid(error.to_string()))?;
    let mut input = tenant_as_input(&current);
    input.binding_kid = Some(kid);
    input.binding_alg = Some(alg.trim().to_string());
    input.binding_key = Some(private_pem);
    validate_tenant(&input, &protocol)?;
    conn.execute(
        "UPDATE target_tenants SET binding_kid = ?1, binding_alg = ?2, binding_key = ?3,
            binding_public_key = ?4
         WHERE id = ?5 AND target_id = ?6",
        params![
            input.binding_kid,
            input.binding_alg,
            input.binding_key,
            public_pem,
            tenant_id,
            target_id,
        ],
    )?;
    get_tenant(conn, target_id, tenant_id)
}

/// Delete a tenant.
///
/// # Errors
///
/// Returns not-found or storage errors.
pub fn delete_tenant(
    conn: &Connection,
    target_id: &str,
    tenant_id: &str,
) -> Result<(), StoreError> {
    let _ = get_tenant(conn, target_id, tenant_id)?;
    conn.execute(
        "DELETE FROM target_tenants WHERE id = ?1 AND target_id = ?2",
        params![tenant_id, target_id],
    )?;
    Ok(())
}

/// List products of a target.
///
/// # Errors
///
/// Returns not-found when the target is missing, or a storage error.
pub fn list_products(conn: &Connection, target_id: &str) -> Result<Vec<Product>, StoreError> {
    ensure_target(conn, target_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, target_id, name, product_id, channel_code, path, method, weight,
                qps_limit, timeout_secs, query_json, headers_json, body_json,
                body_image_dir, body_image_field, body_image_name_field,
                expect_transport, expect_checks_json
         FROM target_products WHERE target_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map(params![target_id], product_from_row)?;
    collect_rows(rows)
}

/// Load one product.
///
/// # Errors
///
/// Returns not-found when the target or product is missing.
pub fn get_product(
    conn: &Connection,
    target_id: &str,
    product_id: &str,
) -> Result<Product, StoreError> {
    list_products(conn, target_id)?
        .into_iter()
        .find(|product| product.id == product_id)
        .ok_or_else(|| StoreError::not_found(format!("product {product_id} not found")))
}

/// Insert a product under a target.
///
/// # Errors
///
/// Returns validation, conflict, or storage errors.
pub fn create_product(
    conn: &Connection,
    target_id: &str,
    input: &ProductInput,
) -> Result<Product, StoreError> {
    ensure_target(conn, target_id)?;
    validate_product(input)?;
    if product_name_taken(conn, target_id, &input.name, None)? {
        return Err(StoreError::conflict(format!(
            "a product named '{}' already exists on this target",
            input.name
        )));
    }
    let id = new_id();
    insert_product_row(conn, &id, target_id, input)?;
    get_product(conn, target_id, &id)
}

/// Replace a product.
///
/// # Errors
///
/// Returns validation, not-found, conflict, or storage errors.
pub fn update_product(
    conn: &Connection,
    target_id: &str,
    product_id: &str,
    input: &ProductInput,
) -> Result<Product, StoreError> {
    let _ = get_product(conn, target_id, product_id)?;
    validate_product(input)?;
    if product_name_taken(conn, target_id, &input.name, Some(product_id))? {
        return Err(StoreError::conflict(format!(
            "a product named '{}' already exists on this target",
            input.name
        )));
    }
    conn.execute(
        "DELETE FROM target_products WHERE id = ?1",
        params![product_id],
    )?;
    insert_product_row(conn, product_id, target_id, input)?;
    get_product(conn, target_id, product_id)
}

/// Delete a product.
///
/// # Errors
///
/// Returns not-found or storage errors.
pub fn delete_product(
    conn: &Connection,
    target_id: &str,
    product_id: &str,
) -> Result<(), StoreError> {
    let _ = get_product(conn, target_id, product_id)?;
    conn.execute(
        "DELETE FROM target_products WHERE id = ?1 AND target_id = ?2",
        params![product_id, target_id],
    )?;
    Ok(())
}

/// Whether a target id exists.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn target_exists(conn: &Connection, id: &str) -> Result<bool, StoreError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM targets WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn target_protocol(conn: &Connection, id: &str) -> Result<String, StoreError> {
    conn.query_row(
        "SELECT protocol FROM targets WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("target {id} not found")))
}

/// Keep a generated public key only while the binding algorithm and private key are unchanged.
fn kept_public_key<'a>(current: &'a Tenant, input: &TenantInput) -> Option<&'a str> {
    let same_alg = opt_str(current.binding_alg.as_deref()) == opt_str(input.binding_alg.as_deref());
    let same_key = opt_str(current.binding_key.as_deref()) == opt_str(input.binding_key.as_deref());
    if same_alg && same_key {
        current.binding_public_key.as_deref()
    } else {
        None
    }
}

fn tenant_as_input(tenant: &Tenant) -> TenantInput {
    TenantInput {
        name: tenant.name.clone(),
        weight: tenant.weight,
        api_key: tenant.api_key.clone().unwrap_or_default(),
        encryption_mode: tenant.encryption_mode.clone(),
        tenant_id: tenant.tenant_id.clone(),
        tenant_key_base64: tenant.tenant_key_base64.clone(),
        key_version: tenant.key_version.clone(),
        suite: tenant.suite.clone(),
        data_alg: tenant.data_alg.clone(),
        mac_alg: tenant.mac_alg.clone(),
        require_signature: tenant.require_signature,
        allow_insecure: tenant.allow_insecure,
        binding_kid: tenant.binding_kid.clone(),
        binding_alg: tenant.binding_alg.clone(),
        binding_key: tenant.binding_key.clone(),
    }
}

fn ensure_target(conn: &Connection, id: &str) -> Result<(), StoreError> {
    if target_exists(conn, id)? {
        Ok(())
    } else {
        Err(StoreError::not_found(format!("target {id} not found")))
    }
}

/// Insert nested tenants and products. A tenant whose name matches one in
/// `previous` keeps its id (and its generated public key while the key is unchanged).
fn insert_nested(
    conn: &Connection,
    target_id: &str,
    tenants: Option<&[TenantInput]>,
    products: Option<&[ProductInput]>,
    previous: &[Tenant],
) -> Result<(), StoreError> {
    if let Some(tenants) = tenants {
        let protocol = target_protocol(conn, target_id)?;
        for tenant in tenants {
            validate_tenant(tenant, &protocol)?;
            let earlier = previous
                .iter()
                .find(|old| old.name.trim() == tenant.name.trim());
            let id = earlier.map_or_else(new_id, |old| old.id.clone());
            let public_key = earlier.and_then(|old| kept_public_key(old, tenant));
            insert_tenant_row(conn, &id, target_id, tenant, public_key)?;
        }
    }
    if let Some(products) = products {
        for product in products {
            validate_product(product)?;
            insert_product_row(conn, &new_id(), target_id, product)?;
        }
    }
    Ok(())
}

fn insert_tenant_row(
    conn: &Connection,
    id: &str,
    target_id: &str,
    input: &TenantInput,
    binding_public_key: Option<&str>,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO target_tenants (
            id, target_id, name, weight, api_key, encryption_mode, tenant_id,
            tenant_key_base64, key_version, suite, data_alg, mac_alg,
            require_signature, allow_insecure, binding_kid, binding_alg, binding_key,
            binding_public_key
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            id,
            target_id,
            input.name.trim(),
            input.weight,
            input.api_key.trim(),
            input.encryption_mode.trim(),
            opt_str(input.tenant_id.as_deref()),
            opt_str(input.tenant_key_base64.as_deref()),
            opt_str(input.key_version.as_deref()),
            opt_str(input.suite.as_deref()),
            opt_str(input.data_alg.as_deref()),
            opt_str(input.mac_alg.as_deref()),
            input.require_signature.map(i64::from),
            i64::from(input.allow_insecure),
            opt_str(input.binding_kid.as_deref()),
            opt_str(input.binding_alg.as_deref()),
            opt_str(input.binding_key.as_deref()),
            binding_public_key,
        ],
    )?;
    Ok(())
}

fn insert_product_row(
    conn: &Connection,
    id: &str,
    target_id: &str,
    input: &ProductInput,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO target_products (
            id, target_id, name, product_id, channel_code, path, method, weight,
            qps_limit, timeout_secs, query_json, headers_json, body_json,
            body_image_dir, body_image_field, body_image_name_field,
            expect_transport, expect_checks_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            id,
            target_id,
            input.name.trim(),
            input.product_id.trim(),
            opt_str(input.channel_code.as_deref()),
            input.path.trim(),
            input.method.trim().to_ascii_uppercase(),
            input.weight,
            input.qps_limit,
            input.timeout_secs,
            json_text(input.query_json.as_ref())?,
            json_text(input.headers_json.as_ref())?,
            json_text(input.body_json.as_ref())?,
            opt_str(input.body_image_dir.as_deref()),
            opt_str(input.body_image_field.as_deref()),
            opt_str(input.body_image_name_field.as_deref()),
            opt_str(input.expect_transport.as_deref()),
            json_text(input.expect_checks_json.as_ref())?,
        ],
    )?;
    Ok(())
}

fn product_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Product> {
    Ok(Product {
        id: row.get(0)?,
        target_id: row.get(1)?,
        name: row.get(2)?,
        product_id: row.get(3)?,
        channel_code: row.get(4)?,
        path: row.get(5)?,
        method: row.get(6)?,
        weight: row.get(7)?,
        qps_limit: row.get(8)?,
        timeout_secs: row.get(9)?,
        query_json: parse_json_cell(row.get(10)?)?,
        headers_json: parse_json_cell(row.get(11)?)?,
        body_json: parse_json_cell(row.get(12)?)?,
        body_image_dir: row.get(13)?,
        body_image_field: row.get(14)?,
        body_image_name_field: row.get(15)?,
        expect_transport: row.get(16)?,
        expect_checks_json: parse_json_cell(row.get(17)?)?,
    })
}

fn parse_json_cell(value: Option<String>) -> rusqlite::Result<Option<Value>> {
    match value {
        None => Ok(None),
        Some(text) if text.is_empty() => Ok(None),
        Some(text) => serde_json::from_str(&text).map(Some).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        }),
    }
}

pub(crate) fn validate_target(input: &TargetInput) -> Result<(), StoreError> {
    require_name(&input.name, "target name")?;
    validate_protocol(&input.protocol)?;
    validate_gateway_url(&input.gateway_base_url)?;
    if let Some(tenants) = input.tenants.as_deref() {
        for tenant in tenants {
            validate_tenant(tenant, input.protocol.trim())?;
        }
    }
    Ok(())
}

fn validate_protocol(raw: &str) -> Result<(), StoreError> {
    match raw.trim() {
        PROTOCOL_HTTP | PROTOCOL_MAZBRI => Ok(()),
        _ => Err(StoreError::invalid("protocol must be http or mazbri")),
    }
}

pub(crate) fn validate_gateway_url(raw: &str) -> Result<(), StoreError> {
    let url = raw.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(StoreError::invalid(
            "gateway_base_url must start with http:// or https://",
        ));
    }
    let rest = url.split_once("://").map_or("", |(_, host)| host);
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    if host.is_empty() || host.starts_with(':') {
        return Err(StoreError::invalid(
            "gateway_base_url must include a host (IP, domain, or hostname)",
        ));
    }
    Ok(())
}

/// Validate a tenant against its target protocol.
///
/// `http` tenants are plaintext (optional `api_key` header only). `mazbri`
/// tenants need an `api_key` and go through the same settings check the engine
/// uses, so suites, algorithms, key lengths, and binding keys are verified
/// when they are saved.
pub(crate) fn validate_tenant(input: &TenantInput, protocol: &str) -> Result<(), StoreError> {
    require_name(&input.name, "tenant name")?;
    if input.weight <= 0 {
        return Err(StoreError::invalid("tenant weight must be greater than 0"));
    }
    if protocol == PROTOCOL_HTTP {
        return validate_http_tenant(input);
    }
    if input.api_key.trim().is_empty() {
        return Err(StoreError::invalid("api_key is required"));
    }
    tenant_settings(input)
        .session()
        .map(drop)
        .map_err(|error| StoreError::invalid(error.to_string()))
}

fn validate_http_tenant(input: &TenantInput) -> Result<(), StoreError> {
    if input.encryption_mode.trim() != "none" {
        return Err(StoreError::invalid(
            "http targets only support encryption_mode none",
        ));
    }
    let mazbri_only = [
        input.tenant_id.as_deref(),
        input.tenant_key_base64.as_deref(),
        input.key_version.as_deref(),
        input.suite.as_deref(),
        input.data_alg.as_deref(),
        input.mac_alg.as_deref(),
        input.binding_kid.as_deref(),
        input.binding_alg.as_deref(),
        input.binding_key.as_deref(),
    ];
    if mazbri_only.iter().any(|value| opt_str(*value).is_some())
        || input.allow_insecure
        || input.require_signature == Some(true)
    {
        return Err(StoreError::invalid(
            "encryption and request binding fields are only used by mazbri targets",
        ));
    }
    Ok(())
}

/// View a tenant input as SDK settings.
#[must_use]
pub fn tenant_settings(input: &TenantInput) -> TenantSettings<'_> {
    TenantSettings {
        encryption_mode: input.encryption_mode.as_str(),
        tenant_id: input.tenant_id.as_deref(),
        tenant_key_base64: input.tenant_key_base64.as_deref(),
        key_version: input.key_version.as_deref(),
        suite: input.suite.as_deref(),
        data_alg: input.data_alg.as_deref(),
        mac_alg: input.mac_alg.as_deref(),
        require_signature: input.require_signature,
        allow_insecure: input.allow_insecure,
        binding_kid: input.binding_kid.as_deref(),
        binding_alg: input.binding_alg.as_deref(),
        binding_key: input.binding_key.as_deref(),
    }
}

pub(crate) fn validate_product(input: &ProductInput) -> Result<(), StoreError> {
    require_name(&input.name, "product name")?;
    if input.product_id.trim().is_empty() {
        return Err(StoreError::invalid("product_id is required"));
    }
    if input.path.trim().is_empty() {
        return Err(StoreError::invalid("product path is required"));
    }
    if input.method.trim().is_empty() {
        return Err(StoreError::invalid("product method is required"));
    }
    if input.weight <= 0 {
        return Err(StoreError::invalid("product weight must be greater than 0"));
    }
    if let Some(limit) = input.qps_limit
        && limit <= 0
    {
        return Err(StoreError::invalid(
            "qps_limit must be greater than 0 when set",
        ));
    }
    if let Some(timeout) = input.timeout_secs
        && timeout <= 0
    {
        return Err(StoreError::invalid(
            "timeout_secs must be greater than 0 when set",
        ));
    }
    Ok(())
}

fn require_name(name: &str, label: &str) -> Result<(), StoreError> {
    if name.trim().is_empty() {
        Err(StoreError::invalid(format!("{label} is required")))
    } else {
        Ok(())
    }
}

fn name_taken(
    conn: &Connection,
    table: &str,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<bool, StoreError> {
    // Table names are only called with literals in this module.
    let sql = if exclude_id.is_some() {
        format!("SELECT COUNT(*) FROM {table} WHERE name = ?1 AND id != ?2")
    } else {
        format!("SELECT COUNT(*) FROM {table} WHERE name = ?1")
    };
    let count: i64 = if let Some(id) = exclude_id {
        conn.query_row(&sql, params![name.trim(), id], |row| row.get(0))?
    } else {
        conn.query_row(&sql, params![name.trim()], |row| row.get(0))?
    };
    Ok(count > 0)
}

fn tenant_name_taken(
    conn: &Connection,
    target_id: &str,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<bool, StoreError> {
    let count: i64 = if let Some(id) = exclude_id {
        conn.query_row(
            "SELECT COUNT(*) FROM target_tenants WHERE target_id = ?1 AND name = ?2 AND id != ?3",
            params![target_id, name.trim(), id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM target_tenants WHERE target_id = ?1 AND name = ?2",
            params![target_id, name.trim()],
            |row| row.get(0),
        )?
    };
    Ok(count > 0)
}

fn product_name_taken(
    conn: &Connection,
    target_id: &str,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<bool, StoreError> {
    let count: i64 = if let Some(id) = exclude_id {
        conn.query_row(
            "SELECT COUNT(*) FROM target_products WHERE target_id = ?1 AND name = ?2 AND id != ?3",
            params![target_id, name.trim(), id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM target_products WHERE target_id = ?1 AND name = ?2",
            params![target_id, name.trim()],
            |row| row.get(0),
        )?
    };
    Ok(count > 0)
}

pub(crate) fn unfinished_run_count(
    conn: &Connection,
    column: &str,
    id: &str,
) -> Result<i64, StoreError> {
    // Column names are only called with literals (`target_id` / `template_id`).
    let sql = format!(
        "SELECT COUNT(*) FROM runs WHERE {column} = ?1 AND status IN ('running', 'stopping', 'interrupted')"
    );
    Ok(conn.query_row(&sql, params![id], |row| row.get(0))?)
}

fn collect_rows<T, E>(rows: impl Iterator<Item = Result<T, E>>) -> Result<Vec<T>, StoreError>
where
    E: Into<StoreError>,
{
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn opt_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn opt_bool_from_i64(value: Option<i64>) -> Option<bool> {
    value.map(|n| n != 0)
}

fn json_text(value: Option<&Value>) -> Result<Option<String>, StoreError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(other) => Ok(Some(serde_json::to_string(other)?)),
    }
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(crate) fn now_ms() -> Result<i64, StoreError> {
    Ok(i64::try_from(unix_now_ms().map_err(anyhow::Error::from)?).unwrap_or(i64::MAX))
}
