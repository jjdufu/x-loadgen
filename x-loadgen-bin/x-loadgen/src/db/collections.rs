//! SQLite persistence and Hoppscotch-compatible import/export for Collections,
//! nested Folders, and Requests.

use std::collections::{BTreeMap, HashMap};

use reqwest::Method;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::error::StoreError;
use super::workspace::{
    AssertionsSpec, EnvVarRow, HttpAuthSpec, KeyValueRow, MazbriRequestSpec, PROTOCOL_MAZBRI,
    RequestBodySpec, TlsSpec, default_http_protocol, new_id, non_empty_owned, now_ms,
    validate_protocol,
};

const fn default_weight() -> i64 {
    1
}

fn default_method_get() -> String {
    "GET".to_string()
}

/// Flat row of the `collections` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub default_protocol: String,
    pub auth: HttpAuthSpec,
    pub headers: Vec<KeyValueRow>,
    pub variables: Vec<EnvVarRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Recursive Hoppscotch-style collection / folder tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionNode {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub default_protocol: String,
    pub auth: HttpAuthSpec,
    pub headers: Vec<KeyValueRow>,
    pub variables: Vec<EnvVarRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub folders: Vec<CollectionNode>,
    pub requests: Vec<SavedRequest>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create/update payload for a collection or folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionInput {
    #[serde(default)]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default = "default_http_protocol")]
    pub default_protocol: String,
    #[serde(default)]
    pub auth: HttpAuthSpec,
    #[serde(default)]
    pub headers: Vec<KeyValueRow>,
    #[serde(default)]
    pub variables: Vec<EnvVarRow>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Saved request item belonging to a collection/folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: String,
    pub collection_id: String,
    pub name: String,
    pub sort_order: i64,
    pub protocol: String,
    pub method: String,
    pub endpoint: String,
    pub params: Vec<KeyValueRow>,
    pub headers: Vec<KeyValueRow>,
    pub body: RequestBodySpec,
    pub auth: HttpAuthSpec,
    pub mazbri: MazbriRequestSpec,
    pub tls: TlsSpec,
    pub assertions: AssertionsSpec,
    pub weight: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl SavedRequest {
    /// Convert into an inline [`RequestDraft`].
    #[must_use]
    pub fn to_draft(&self) -> RequestDraft {
        RequestDraft {
            id: Some(self.id.clone()),
            collection_id: Some(self.collection_id.clone()),
            name: self.name.clone(),
            sort_order: Some(self.sort_order),
            protocol: self.protocol.clone(),
            method: self.method.clone(),
            endpoint: self.endpoint.clone(),
            params: self.params.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            auth: self.auth.clone(),
            mazbri: self.mazbri.clone(),
            tls: self.tls.clone(),
            assertions: self.assertions.clone(),
            weight: self.weight,
            timeout_secs: self.timeout_secs,
        }
    }

    /// Return a redacted copy safe for non-secret export.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            auth: self.auth.redacted(),
            mazbri: self.mazbri.redacted(),
            tls: self.tls.redacted(),
            ..self.clone()
        }
    }
}

/// Create/update or ad-hoc execution payload for a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(default = "default_http_protocol")]
    pub protocol: String,
    #[serde(default = "default_method_get")]
    pub method: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub params: Vec<KeyValueRow>,
    #[serde(default)]
    pub headers: Vec<KeyValueRow>,
    #[serde(default)]
    pub body: RequestBodySpec,
    #[serde(default)]
    pub auth: HttpAuthSpec,
    #[serde(default)]
    pub mazbri: MazbriRequestSpec,
    #[serde(default)]
    pub tls: TlsSpec,
    #[serde(default)]
    pub assertions: AssertionsSpec,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i64>,
}

impl Default for RequestDraft {
    fn default() -> Self {
        Self {
            id: None,
            collection_id: None,
            name: "Untitled Request".to_string(),
            sort_order: None,
            protocol: default_http_protocol(),
            method: default_method_get(),
            endpoint: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body: RequestBodySpec::default(),
            auth: HttpAuthSpec::default(),
            mazbri: MazbriRequestSpec::default(),
            tls: TlsSpec::default(),
            assertions: AssertionsSpec::default(),
            weight: 1,
            timeout_secs: None,
        }
    }
}

impl RequestDraft {
    /// Normalize and validate fields for persistence or execution.
    ///
    /// # Errors
    ///
    /// Returns validation error when method, protocol, weight, timeout, auth, TLS,
    /// assertions, or Mazbri settings are invalid.
    pub fn validate(&self, require_collection: bool) -> Result<NormalizedRequest, StoreError> {
        let name = if self.name.trim().is_empty() {
            "Untitled Request".to_string()
        } else {
            self.name.trim().to_string()
        };
        let collection_id = non_empty_owned(self.collection_id.as_deref());
        if require_collection && collection_id.is_none() {
            return Err(StoreError::invalid("collection_id is required"));
        }
        let protocol = validate_protocol(&self.protocol)?;
        let method_upper = self.method.trim().to_ascii_uppercase();
        if Method::from_bytes(method_upper.as_bytes()).is_err() {
            return Err(StoreError::invalid(format!(
                "invalid HTTP method {:?}",
                self.method
            )));
        }
        if self.weight <= 0 {
            return Err(StoreError::invalid("request weight must be > 0"));
        }
        if let Some(t) = self.timeout_secs
            && t <= 0
        {
            return Err(StoreError::invalid("timeout_secs must be > 0 when set"));
        }
        self.auth.validate()?;
        self.tls.validate()?;
        self.assertions.validate()?;
        let mut mazbri = self.mazbri.clone();
        if protocol == PROTOCOL_MAZBRI {
            mazbri.validate()?;
            if mazbri.product_id.trim().is_empty() {
                mazbri.product_id = slugify_product_id(&name);
            } else {
                mazbri.product_id = mazbri.product_id.trim().to_string();
            }
        }
        Ok(NormalizedRequest {
            collection_id,
            name,
            sort_order: self.sort_order.unwrap_or(0),
            protocol,
            method: method_upper,
            endpoint: self.endpoint.trim().to_string(),
            params: self.params.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            auth: self.auth.clone(),
            mazbri,
            tls: self.tls.clone(),
            assertions: self.assertions.clone(),
            weight: self.weight,
            timeout_secs: self.timeout_secs,
        })
    }
}

/// Validated request fields ready for DB insert/update or execution compilation.
#[derive(Debug, Clone)]
pub struct NormalizedRequest {
    pub collection_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub protocol: String,
    pub method: String,
    pub endpoint: String,
    pub params: Vec<KeyValueRow>,
    pub headers: Vec<KeyValueRow>,
    pub body: RequestBodySpec,
    pub auth: HttpAuthSpec,
    pub mazbri: MazbriRequestSpec,
    pub tls: TlsSpec,
    pub assertions: AssertionsSpec,
    pub weight: i64,
    pub timeout_secs: Option<i64>,
}

fn slugify_product_id(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() && !out.ends_with('.') {
            out.push('.');
        }
    }
    let trimmed = out.trim_matches('.').to_string();
    if trimmed.is_empty() {
        "default.product".to_string()
    } else {
        trimmed
    }
}

/// Create a collection or folder.
///
/// # Errors
///
/// Returns validation/not-found/storage error.
pub fn create_collection(
    conn: &Connection,
    input: &CollectionInput,
) -> Result<CollectionRecord, StoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("collection name must not be empty"));
    }
    let parent_id = non_empty_owned(input.parent_id.as_deref());
    if let Some(pid) = parent_id.as_deref() {
        let _ = get_collection(conn, pid)?;
    }
    let default_protocol = validate_protocol(&input.default_protocol)?;
    input.auth.validate()?;
    let id = new_id();
    let now = now_ms()?;
    let sort_order = match input.sort_order {
        Some(order) => order,
        None => next_collection_order(conn, parent_id.as_deref())?,
    };
    let auth_json = serde_json::to_string(&input.auth).unwrap_or_else(|_| "{}".into());
    let headers_json = serde_json::to_string(&input.headers).unwrap_or_else(|_| "[]".into());
    let variables_json = serde_json::to_string(&input.variables).unwrap_or_else(|_| "[]".into());
    let description = non_empty_owned(input.description.as_deref());
    conn.execute(
        "INSERT INTO collections (
            id, parent_id, name, sort_order, default_protocol,
            auth_json, headers_json, variables_json, description, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            id,
            parent_id,
            name,
            sort_order,
            default_protocol,
            auth_json,
            headers_json,
            variables_json,
            description,
            now
        ],
    )?;
    get_collection(conn, &id)
}

/// Update a collection or folder (detects parent cycles).
///
/// # Errors
///
/// Returns validation/not-found/storage error.
pub fn update_collection(
    conn: &Connection,
    id: &str,
    input: &CollectionInput,
) -> Result<CollectionRecord, StoreError> {
    let existing = get_collection(conn, id)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("collection name must not be empty"));
    }
    let parent_id = non_empty_owned(input.parent_id.as_deref());
    if let Some(pid) = parent_id.as_deref() {
        if pid == id {
            return Err(StoreError::invalid("a collection cannot be its own parent"));
        }
        let ancestors = ancestor_chain(conn, pid)?;
        if ancestors.iter().any(|c| c.id == id) {
            return Err(StoreError::invalid(
                "moving collection under its own descendant is not allowed",
            ));
        }
    }
    let default_protocol = validate_protocol(&input.default_protocol)?;
    input.auth.validate()?;
    let now = now_ms()?;
    let sort_order = input.sort_order.unwrap_or(existing.sort_order);
    let auth_json = serde_json::to_string(&input.auth).unwrap_or_else(|_| "{}".into());
    let headers_json = serde_json::to_string(&input.headers).unwrap_or_else(|_| "[]".into());
    let variables_json = serde_json::to_string(&input.variables).unwrap_or_else(|_| "[]".into());
    let description = non_empty_owned(input.description.as_deref());
    conn.execute(
        "UPDATE collections SET
            parent_id = ?1, name = ?2, sort_order = ?3, default_protocol = ?4,
            auth_json = ?5, headers_json = ?6, variables_json = ?7,
            description = ?8, updated_at = ?9
         WHERE id = ?10",
        params![
            parent_id,
            name,
            sort_order,
            default_protocol,
            auth_json,
            headers_json,
            variables_json,
            description,
            now,
            id
        ],
    )?;
    get_collection(conn, id)
}

/// Delete a collection (cascades to nested folders and requests).
///
/// # Errors
///
/// Returns not-found if `id` does not exist.
pub fn delete_collection(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let changed = conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(StoreError::not_found(format!("collection {id} not found")));
    }
    Ok(())
}

/// Load a single collection record.
///
/// # Errors
///
/// Returns not-found if `id` does not exist.
pub fn get_collection(conn: &Connection, id: &str) -> Result<CollectionRecord, StoreError> {
    conn.query_row(
        "SELECT id, parent_id, name, sort_order, default_protocol,
                auth_json, headers_json, variables_json, description, created_at, updated_at
         FROM collections WHERE id = ?1",
        params![id],
        collection_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("collection {id} not found")))
}

/// Load the ancestor chain from root collection down to `collection_id` (inclusive).
///
/// # Errors
///
/// Returns not-found if `collection_id` does not exist.
pub fn ancestor_chain(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<CollectionRecord>, StoreError> {
    let mut chain = Vec::new();
    let mut cursor = Some(collection_id.to_string());
    let mut guard = 0_usize;
    while let Some(cid) = cursor.take() {
        guard += 1;
        if guard > 64 {
            break;
        }
        let rec = get_collection(conn, &cid)?;
        cursor.clone_from(&rec.parent_id);
        chain.push(rec);
    }
    chain.reverse();
    Ok(chain)
}

/// Load the full nested collection/folder/request tree.
///
/// # Errors
///
/// Returns a storage error if queries fail.
pub fn list_collection_tree(conn: &Connection) -> Result<Vec<CollectionNode>, StoreError> {
    let mut col_stmt = conn.prepare(
        "SELECT id, parent_id, name, sort_order, default_protocol,
                auth_json, headers_json, variables_json, description, created_at, updated_at
         FROM collections ORDER BY sort_order ASC, created_at ASC",
    )?;
    let cols: Vec<CollectionRecord> = col_stmt
        .query_map([], collection_from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    let mut req_stmt = conn.prepare(
        "SELECT id, collection_id, name, sort_order, protocol, method, endpoint,
                params_json, headers_json, body_json, auth_json, mazbri_json,
                tls_json, assertions_json, weight, timeout_secs, created_at, updated_at
         FROM requests ORDER BY sort_order ASC, created_at ASC",
    )?;
    let reqs: Vec<SavedRequest> = req_stmt
        .query_map([], request_from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    let mut reqs_by_col: HashMap<String, Vec<SavedRequest>> = HashMap::new();
    for req in reqs {
        reqs_by_col
            .entry(req.collection_id.clone())
            .or_default()
            .push(req);
    }

    let mut children_by_parent: HashMap<Option<String>, Vec<CollectionRecord>> = HashMap::new();
    for col in cols {
        children_by_parent
            .entry(col.parent_id.clone())
            .or_default()
            .push(col);
    }

    Ok(build_nodes(None, &mut children_by_parent, &mut reqs_by_col))
}

fn build_nodes(
    parent_id: Option<&str>,
    children_by_parent: &mut HashMap<Option<String>, Vec<CollectionRecord>>,
    reqs_by_col: &mut HashMap<String, Vec<SavedRequest>>,
) -> Vec<CollectionNode> {
    let lookup_key = parent_id.map(str::to_string);
    let Some(records) = children_by_parent.remove(&lookup_key) else {
        return Vec::new();
    };
    records
        .into_iter()
        .map(|rec| {
            let folders = build_nodes(Some(&rec.id), children_by_parent, reqs_by_col);
            let requests = reqs_by_col.remove(&rec.id).unwrap_or_default();
            CollectionNode {
                id: rec.id,
                parent_id: rec.parent_id,
                name: rec.name,
                sort_order: rec.sort_order,
                default_protocol: rec.default_protocol,
                auth: rec.auth,
                headers: rec.headers,
                variables: rec.variables,
                description: rec.description,
                folders,
                requests,
                created_at: rec.created_at,
                updated_at: rec.updated_at,
            }
        })
        .collect()
}

/// Find a single subtree rooted at `collection_id`.
///
/// # Errors
///
/// Returns not-found if `collection_id` does not exist.
pub fn get_collection_subtree(
    conn: &Connection,
    collection_id: &str,
) -> Result<CollectionNode, StoreError> {
    let _ = get_collection(conn, collection_id)?;
    let tree = list_collection_tree(conn)?;
    find_in_tree(tree, collection_id)
        .ok_or_else(|| StoreError::not_found(format!("collection {collection_id} not found")))
}

fn find_in_tree(nodes: Vec<CollectionNode>, id: &str) -> Option<CollectionNode> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_in_tree(node.folders, id) {
            return Some(found);
        }
    }
    None
}

/// Flatten every [`SavedRequest`] inside `node` and its descendant folders in tree order.
#[must_use]
pub fn flatten_subtree_requests(node: &CollectionNode) -> Vec<SavedRequest> {
    let mut out = node.requests.clone();
    for folder in &node.folders {
        out.extend(flatten_subtree_requests(folder));
    }
    out
}

/// Create a saved request inside a collection/folder.
///
/// # Errors
///
/// Returns validation/not-found/storage error.
pub fn create_request(conn: &Connection, draft: &RequestDraft) -> Result<SavedRequest, StoreError> {
    let norm = draft.validate(true)?;
    let collection_id = norm.collection_id.as_deref().unwrap_or_default();
    let _ = get_collection(conn, collection_id)?;
    let id = new_id();
    let now = now_ms()?;
    let sort_order = match draft.sort_order {
        Some(order) => order,
        None => next_request_order(conn, collection_id)?,
    };
    conn.execute(
        "INSERT INTO requests (
            id, collection_id, name, sort_order, protocol, method, endpoint,
            params_json, headers_json, body_json, auth_json, mazbri_json,
            tls_json, assertions_json, weight, timeout_secs, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
        params![
            id,
            collection_id,
            norm.name,
            sort_order,
            norm.protocol,
            norm.method,
            norm.endpoint,
            serde_json::to_string(&norm.params).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&norm.headers).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&norm.body).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.auth).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.mazbri).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.tls).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.assertions).unwrap_or_else(|_| "{}".into()),
            norm.weight,
            norm.timeout_secs,
            now
        ],
    )?;
    get_request(conn, &id)
}

/// Update an existing saved request.
///
/// # Errors
///
/// Returns validation/not-found/storage error.
pub fn update_request(
    conn: &Connection,
    id: &str,
    draft: &RequestDraft,
) -> Result<SavedRequest, StoreError> {
    let existing = get_request(conn, id)?;
    let mut merged = draft.clone();
    if merged.collection_id.is_none() {
        merged.collection_id = Some(existing.collection_id.clone());
    }
    // Preserve generated inline binding_public_key when binding_alg & binding_key did not change.
    if let (Some(old_inline), Some(new_inline)) = (
        existing.mazbri.inline_tenant.as_ref(),
        merged.mazbri.inline_tenant.as_mut(),
    ) && new_inline.binding_public_key.is_none()
        && old_inline.binding_alg == new_inline.binding_alg
        && old_inline.binding_key == new_inline.binding_key
    {
        new_inline
            .binding_public_key
            .clone_from(&old_inline.binding_public_key);
    }
    let norm = merged.validate(true)?;
    let collection_id = norm.collection_id.as_deref().unwrap_or_default();
    let _ = get_collection(conn, collection_id)?;
    let now = now_ms()?;
    let sort_order = draft.sort_order.unwrap_or(existing.sort_order);
    conn.execute(
        "UPDATE requests SET
            collection_id = ?1, name = ?2, sort_order = ?3, protocol = ?4, method = ?5,
            endpoint = ?6, params_json = ?7, headers_json = ?8, body_json = ?9,
            auth_json = ?10, mazbri_json = ?11, tls_json = ?12, assertions_json = ?13,
            weight = ?14, timeout_secs = ?15, updated_at = ?16
         WHERE id = ?17",
        params![
            collection_id,
            norm.name,
            sort_order,
            norm.protocol,
            norm.method,
            norm.endpoint,
            serde_json::to_string(&norm.params).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&norm.headers).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&norm.body).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.auth).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.mazbri).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.tls).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&norm.assertions).unwrap_or_else(|_| "{}".into()),
            norm.weight,
            norm.timeout_secs,
            now,
            id
        ],
    )?;
    get_request(conn, id)
}

/// Delete a saved request.
///
/// # Errors
///
/// Returns not-found when `id` is unknown.
pub fn delete_request(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let changed = conn.execute("DELETE FROM requests WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(StoreError::not_found(format!("request {id} not found")));
    }
    Ok(())
}

/// Fetch one saved request by id.
///
/// # Errors
///
/// Returns not-found when `id` is unknown.
pub fn get_request(conn: &Connection, id: &str) -> Result<SavedRequest, StoreError> {
    conn.query_row(
        "SELECT id, collection_id, name, sort_order, protocol, method, endpoint,
                params_json, headers_json, body_json, auth_json, mazbri_json,
                tls_json, assertions_json, weight, timeout_secs, created_at, updated_at
         FROM requests WHERE id = ?1",
        params![id],
        request_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("request {id} not found")))
}

/// Merge variables across an ancestor collection chain (`root -> ... -> leaf`).
#[must_use]
pub fn merge_collection_variables(chain: &[CollectionRecord]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for col in chain {
        for var in &col.variables {
            if var.active && !var.key.trim().is_empty() {
                out.insert(var.key.trim().to_string(), var.value.clone());
            }
        }
    }
    out
}

/// Resolve inherited HTTP auth across `[Environment, RootCollection, ..., LeafCollection, Request]`.
#[must_use]
pub fn resolve_inherited_auth(
    env_auth: Option<&HttpAuthSpec>,
    chain: &[CollectionRecord],
    req_auth: &HttpAuthSpec,
) -> HttpAuthSpec {
    if !req_auth.auth_active {
        return HttpAuthSpec::none();
    }
    if req_auth.auth_type != "inherit" {
        return req_auth.clone();
    }
    for col in chain.iter().rev() {
        if !col.auth.auth_active {
            return HttpAuthSpec::none();
        }
        if col.auth.auth_type != "inherit" {
            return col.auth.clone();
        }
    }
    if let Some(env) = env_auth {
        if !env.auth_active || env.auth_type == "inherit" {
            return HttpAuthSpec::none();
        }
        return env.clone();
    }
    HttpAuthSpec::none()
}

/// Merge active headers across `chain` followed by `req_headers`.
/// Later headers with the same case-insensitive key override inherited ones,
/// while preserving multi-value headers within `req_headers`.
#[must_use]
pub fn merge_inherited_headers(
    chain: &[CollectionRecord],
    req_headers: &[KeyValueRow],
) -> Vec<KeyValueRow> {
    let mut inherited: Vec<KeyValueRow> = Vec::new();
    for col in chain {
        for row in &col.headers {
            if !row.active || row.key.trim().is_empty() {
                continue;
            }
            inherited.retain(|existing| !existing.key.eq_ignore_ascii_case(row.key.trim()));
            inherited.push(row.clone());
        }
    }
    for row in req_headers {
        if row.active && !row.key.trim().is_empty() {
            inherited.retain(|existing| !existing.key.eq_ignore_ascii_case(row.key.trim()));
        }
    }
    inherited.extend(
        req_headers
            .iter()
            .filter(|r| r.active && !r.key.trim().is_empty())
            .cloned(),
    );
    inherited
}

/// Export collections in a portable JSON format (redacted by default unless `include_secrets` is true).
///
/// # Errors
///
/// Returns not-found/storage error.
pub fn export_collections_json(
    conn: &Connection,
    collection_id: Option<&str>,
    include_secrets: bool,
) -> Result<Value, StoreError> {
    let mut nodes = match collection_id {
        Some(id) => vec![get_collection_subtree(conn, id)?],
        None => list_collection_tree(conn)?,
    };
    if !include_secrets {
        for node in &mut nodes {
            redact_node(node);
        }
    }
    Ok(json!({
        "format": "x-loadgen-collections",
        "version": 2,
        "collections": nodes,
    }))
}

fn redact_node(node: &mut CollectionNode) {
    node.auth = node.auth.redacted();
    node.variables = node.variables.iter().map(EnvVarRow::redacted).collect();
    for req in &mut node.requests {
        *req = req.redacted();
    }
    for folder in &mut node.folders {
        redact_node(folder);
    }
}

/// Import either native `x-loadgen` Collection JSON OR standard Hoppscotch REST Collection JSON!
///
/// # Errors
///
/// Returns validation or storage error if the JSON structure is invalid.
pub fn import_collections_json(
    conn: &Connection,
    payload: &Value,
    parent_id: Option<&str>,
) -> Result<Vec<CollectionNode>, StoreError> {
    if let Some(pid) = parent_id {
        let _ = get_collection(conn, pid)?;
    }
    let items: Vec<&Value> = if let Some(arr) = payload.as_array() {
        arr.iter().collect()
    } else if let Some(arr) = payload.get("collections").and_then(Value::as_array) {
        arr.iter().collect()
    } else if payload.is_object() && payload.get("name").is_some() {
        vec![payload]
    } else {
        return Err(StoreError::invalid(
            "expected a Hoppscotch collection object/array or x-loadgen collections bundle",
        ));
    };

    let mut created_ids = Vec::new();
    for item in items {
        let id = import_one_node(conn, item, parent_id)?;
        created_ids.push(id);
    }
    let mut out = Vec::new();
    for id in created_ids {
        out.push(get_collection_subtree(conn, &id)?);
    }
    Ok(out)
}

fn import_one_node(
    conn: &Connection,
    item: &Value,
    parent_id: Option<&str>,
) -> Result<String, StoreError> {
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Imported Collection");
    let default_protocol = item
        .get("default_protocol")
        .and_then(Value::as_str)
        .unwrap_or("http")
        .to_string();
    let auth = item
        .get("auth")
        .map(parse_hopp_or_native_auth)
        .unwrap_or_default();
    let headers: Vec<KeyValueRow> = item
        .get("headers")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let variables: Vec<EnvVarRow> = item
        .get("variables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let description = item
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);

    let created = create_collection(
        conn,
        &CollectionInput {
            parent_id: parent_id.map(str::to_string),
            name: name.to_string(),
            sort_order: None,
            default_protocol,
            auth,
            headers,
            variables,
            description,
        },
    )?;

    if let Some(requests) = item.get("requests").and_then(Value::as_array) {
        for req_val in requests {
            let draft = parse_hopp_or_native_request(req_val, &created.id);
            let _ = create_request(conn, &draft)?;
        }
    }
    if let Some(folders) = item.get("folders").and_then(Value::as_array) {
        for folder_val in folders {
            let _ = import_one_node(conn, folder_val, Some(&created.id))?;
        }
    }
    Ok(created.id)
}

fn parse_hopp_or_native_auth(val: &Value) -> HttpAuthSpec {
    let auth_type = val
        .get("auth_type")
        .or_else(|| val.get("authType"))
        .and_then(Value::as_str)
        .unwrap_or("inherit");
    let normalized_type = match auth_type {
        "none" | "inherit" | "bearer" | "basic" | "api-key" => auth_type.to_string(),
        _ => "none".to_string(),
    };
    let auth_active = val
        .get("auth_active")
        .or_else(|| val.get("authActive"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let add_to = val
        .get("add_to")
        .or_else(|| val.get("addTo"))
        .and_then(Value::as_str)
        .map(|s| {
            if s.eq_ignore_ascii_case("QUERY_PARAMS") {
                "QUERY_PARAMS".to_string()
            } else {
                "HEADERS".to_string()
            }
        });
    HttpAuthSpec {
        auth_type: normalized_type,
        auth_active,
        token: val.get("token").and_then(Value::as_str).map(str::to_string),
        username: val
            .get("username")
            .and_then(Value::as_str)
            .map(str::to_string),
        password: val
            .get("password")
            .and_then(Value::as_str)
            .map(str::to_string),
        key: val.get("key").and_then(Value::as_str).map(str::to_string),
        value: val.get("value").and_then(Value::as_str).map(str::to_string),
        add_to,
    }
}

fn parse_hopp_or_native_request(val: &Value, collection_id: &str) -> RequestDraft {
    let name = val
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Imported Request")
        .to_string();
    let protocol = val
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("http")
        .to_string();
    let method = val
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_string();
    let endpoint = val
        .get("endpoint")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let params: Vec<KeyValueRow> = val
        .get("params")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let headers: Vec<KeyValueRow> = val
        .get("headers")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let body = parse_hopp_or_native_body(val.get("body"));
    let auth = val
        .get("auth")
        .map(parse_hopp_or_native_auth)
        .unwrap_or_default();
    let mazbri: MazbriRequestSpec = val
        .get("mazbri")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let tls: TlsSpec = val
        .get("tls")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let assertions: AssertionsSpec = val
        .get("assertions")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let weight = val
        .get("weight")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let timeout_secs = val.get("timeout_secs").and_then(Value::as_i64);
    RequestDraft {
        id: None,
        collection_id: Some(collection_id.to_string()),
        name,
        sort_order: None,
        protocol,
        method,
        endpoint,
        params,
        headers,
        body,
        auth,
        mazbri,
        tls,
        assertions,
        weight,
        timeout_secs,
    }
}

fn parse_hopp_or_native_body(body_val: Option<&Value>) -> RequestBodySpec {
    let Some(val) = body_val else {
        return RequestBodySpec::default();
    };
    let content_type = val
        .get("content_type")
        .or_else(|| val.get("contentType"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut form: Vec<KeyValueRow> = val
        .get("form")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // Hoppscotch multipart stores array under `body` when contentType == multipart/form-data.
    let body_str = match val.get("body") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(arr)) => {
            if form.is_empty() {
                for entry in arr {
                    if entry.get("isFile").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    let key = entry
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let value = entry
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let active = entry.get("active").and_then(Value::as_bool).unwrap_or(true);
                    form.push(KeyValueRow {
                        key,
                        value,
                        active,
                        description: None,
                    });
                }
            }
            None
        }
        Some(other) if !other.is_null() => Some(other.to_string()),
        _ => None,
    };
    RequestBodySpec {
        content_type,
        body: body_str,
        form,
    }
}

fn next_collection_order(conn: &Connection, parent_id: Option<&str>) -> Result<i64, StoreError> {
    let max_order: Option<i64> = match parent_id {
        Some(pid) => conn.query_row(
            "SELECT MAX(sort_order) FROM collections WHERE parent_id = ?1",
            params![pid],
            |row| row.get(0),
        )?,
        None => conn.query_row(
            "SELECT MAX(sort_order) FROM collections WHERE parent_id IS NULL",
            [],
            |row| row.get(0),
        )?,
    };
    Ok(max_order.map_or(0, |n| n.saturating_add(1)))
}

fn next_request_order(conn: &Connection, collection_id: &str) -> Result<i64, StoreError> {
    let max_order: Option<i64> = conn.query_row(
        "SELECT MAX(sort_order) FROM requests WHERE collection_id = ?1",
        params![collection_id],
        |row| row.get(0),
    )?;
    Ok(max_order.map_or(0, |n| n.saturating_add(1)))
}

fn collection_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectionRecord> {
    let auth_raw: String = row.get(5)?;
    let headers_raw: String = row.get(6)?;
    let vars_raw: String = row.get(7)?;
    Ok(CollectionRecord {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        name: row.get(2)?,
        sort_order: row.get(3)?,
        default_protocol: row.get(4)?,
        auth: serde_json::from_str(&auth_raw).unwrap_or_default(),
        headers: serde_json::from_str(&headers_raw).unwrap_or_default(),
        variables: serde_json::from_str(&vars_raw).unwrap_or_default(),
        description: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn request_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedRequest> {
    let params_raw: String = row.get(7)?;
    let headers_raw: String = row.get(8)?;
    let body_raw: String = row.get(9)?;
    let auth_raw: String = row.get(10)?;
    let mazbri_raw: String = row.get(11)?;
    let tls_raw: String = row.get(12)?;
    let assertions_raw: String = row.get(13)?;
    Ok(SavedRequest {
        id: row.get(0)?,
        collection_id: row.get(1)?,
        name: row.get(2)?,
        sort_order: row.get(3)?,
        protocol: row.get(4)?,
        method: row.get(5)?,
        endpoint: row.get(6)?,
        params: serde_json::from_str(&params_raw).unwrap_or_default(),
        headers: serde_json::from_str(&headers_raw).unwrap_or_default(),
        body: serde_json::from_str(&body_raw).unwrap_or_default(),
        auth: serde_json::from_str(&auth_raw).unwrap_or_default(),
        mazbri: serde_json::from_str(&mazbri_raw).unwrap_or_default(),
        tls: serde_json::from_str(&tls_raw).unwrap_or_default(),
        assertions: serde_json::from_str(&assertions_raw).unwrap_or_default(),
        weight: row.get(14)?,
        timeout_secs: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
