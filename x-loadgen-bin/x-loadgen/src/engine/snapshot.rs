//! Compiles Hoppscotch workspace entities (Saved Request, Collection/Folder, or
//! Ad-hoc Tab Draft + Global/Named Environment + Load Profile) into a frozen,
//! self-contained [`RunSnapshot`] for single-process and multi-process workers.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::StoreError;
use crate::db::collections::{
    CollectionRecord, NormalizedRequest, RequestDraft, SavedRequest, ancestor_chain,
    flatten_subtree_requests, get_collection_subtree, get_request, merge_collection_variables,
    merge_inherited_headers, resolve_inherited_auth,
};
use crate::db::environments::{
    EnvironmentDetail, ensure_global_environment, resolve_environment_context,
};
use crate::db::profiles::{LoadConfig, get_load_profile_by_id_or_name};
use crate::db::runs::CreateRunRequest;
use crate::db::workspace::{
    AssertionsSpec, HttpAuthSpec, KeyValueRow, MazbriRequestSpec, PROTOCOL_HTTP, PROTOCOL_MAZBRI,
    RequestBodySpec, TenantSpec, TlsSpec, new_id, non_empty_owned, non_empty_str,
    validate_http_url,
};
use crate::run_assert::AssertSpec;
use crate::vars::{RuntimeVars, render_str};

/// Metadata describing what triggered a load-test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSourceMeta {
    /// `"request"`, `"collection"`, or `"adhoc"`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_profile_name: Option<String>,
}

/// One resolved, self-contained request inside an [`ExecutionTarget`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub id: String,
    pub name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i64>,
    /// Merged static variables (`Global < Environment < Ancestor Collections < Override`).
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    /// Variable keys marked `secret = true` in Global, Environment, or Collection scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_variables: Vec<String>,
}

impl ExecutionRequest {
    /// Display product identifier recorded in `samples.product_id`.
    #[must_use]
    pub fn sample_product_id(&self) -> String {
        if self.protocol == PROTOCOL_MAZBRI && !self.mazbri.product_id.trim().is_empty() {
            let vars = RuntimeVars::for_index(0);
            render_str(self.mazbri.product_id.trim(), &self.variables, &vars)
        } else {
            format!("{} {}", self.method, self.name)
        }
    }
}

/// Frozen target context for an execution (single request, collection, or ad-hoc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTarget {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_base_url: Option<String>,
    #[serde(default)]
    pub tls: TlsSpec,
    #[serde(default)]
    pub tenants: Vec<TenantSpec>,
    pub requests: Vec<ExecutionRequest>,
}

/// Complete frozen run snapshot persisted in `runs.snapshot_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub title: String,
    pub source: RunSourceMeta,
    pub target: ExecutionTarget,
    pub template: LoadConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assert: Option<AssertSpec>,
}

/// Compile a single [`ExecutionTarget`] (with 1+ requests) from workspace state.
/// Also reused by single-request Probe (`POST /api/v1/probe`).
///
/// # Errors
///
/// Returns validation or not-found errors when referenced IDs are missing or
/// effective URLs / Mazbri credentials are incomplete.
pub fn compile_single_request_target(
    conn: &Connection,
    request_id: Option<&str>,
    draft: Option<&RequestDraft>,
    collection_id_hint: Option<&str>,
    environment_id: Option<&str>,
    variables_override: Option<&BTreeMap<String, String>>,
) -> Result<(ExecutionTarget, Option<String>, Option<String>), StoreError> {
    let global_env = ensure_global_environment(conn)?;
    let (env_detail, base_vars) = resolve_environment_context(conn, environment_id)?;
    let base_secrets = base_secret_var_keys(&global_env, env_detail.as_ref());

    let (resolved_req_id, resolved_col_id, raw_draft) = match (non_empty_str(request_id), draft) {
        (Some(rid), None) => {
            let saved = get_request(conn, rid)?;
            let cid = saved.collection_id.clone();
            (Some(saved.id.clone()), Some(cid), saved.to_draft())
        }
        (rid_opt, Some(d)) => {
            let cid = non_empty_owned(collection_id_hint)
                .or_else(|| non_empty_owned(d.collection_id.as_deref()));
            (rid_opt.map(str::to_string), cid, d.clone())
        }
        (None, None) => {
            return Err(StoreError::invalid(
                "either request_id or inline request draft must be provided",
            ));
        }
    };

    let chain = match resolved_col_id.as_deref() {
        Some(cid) => ancestor_chain(conn, cid)?,
        None => Vec::new(),
    };
    let norm = raw_draft.validate(false)?;
    let exec_req = build_execution_request(
        resolved_req_id.clone().unwrap_or_else(new_id),
        &norm,
        &chain,
        env_detail.as_ref().map(|e| &e.http_auth),
        &base_vars,
        &base_secrets,
        variables_override,
    );

    let target_base_url = env_detail.as_ref().and_then(|e| e.gateway_base_url.clone());
    let target_tls = env_detail
        .as_ref()
        .map(|e| e.tls.clone())
        .unwrap_or_default();
    let target_tenants = env_detail
        .as_ref()
        .map(|e| e.tenants.clone())
        .unwrap_or_default();

    validate_compiled_request(
        &exec_req,
        target_base_url.as_deref(),
        &target_tls,
        &target_tenants,
    )?;

    let target = ExecutionTarget {
        id: resolved_req_id
            .clone()
            .unwrap_or_else(|| "adhoc".to_string()),
        name: exec_req.name.clone(),
        environment_id: env_detail.as_ref().map(|e| e.id.clone()),
        environment_name: env_detail.as_ref().map(|e| e.name.clone()),
        gateway_base_url: target_base_url,
        tls: target_tls,
        tenants: target_tenants,
        requests: vec![exec_req],
    };

    Ok((target, resolved_req_id, resolved_col_id))
}

struct CompileEnvCtx<'a> {
    env_detail: Option<&'a EnvironmentDetail>,
    base_vars: &'a BTreeMap<String, String>,
    base_secrets: &'a BTreeSet<String>,
    target_base_url: Option<&'a str>,
    target_tls: &'a TlsSpec,
    target_tenants: &'a [TenantSpec],
}

type CompiledSource = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Vec<ExecutionRequest>,
);

/// Compile a full [`RunSnapshot`] from [`CreateRunRequest`].
///
/// # Errors
///
/// Returns validation or not-found errors when inputs or caps fail validation.
pub fn compile_run_snapshot(
    conn: &Connection,
    create: &CreateRunRequest,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<RunSnapshot, StoreError> {
    let (mut load_config, profile_id, profile_name, profile_assert) = match create.profile_key() {
        Some(key) => {
            let profile = get_load_profile_by_id_or_name(conn, key)?;
            (
                profile.config,
                Some(profile.id),
                Some(profile.name),
                profile.assert,
            )
        }
        None => (LoadConfig::default(), None, None, None),
    };
    create.apply_to(&mut load_config)?;
    load_config.validate(max_qps, max_concurrency)?;

    let assert = create.assert_spec(profile_assert.as_ref());
    if let Some(spec) = &assert {
        spec.validate()?;
        if spec.min_qps_ratio.is_some() && load_config.qps.is_none() {
            return Err(StoreError::invalid(
                "min_qps_ratio requires a configured qps",
            ));
        }
    }

    let global_env = ensure_global_environment(conn)?;
    let (env_detail, base_vars) =
        resolve_environment_context(conn, create.environment_id.as_deref())?;
    let base_secrets = base_secret_var_keys(&global_env, env_detail.as_ref());
    let target_base_url = env_detail.as_ref().and_then(|e| e.gateway_base_url.clone());
    let target_tls = env_detail
        .as_ref()
        .map(|e| e.tls.clone())
        .unwrap_or_default();
    let target_tenants = env_detail
        .as_ref()
        .map(|e| e.tenants.clone())
        .unwrap_or_default();

    let env_ctx = CompileEnvCtx {
        env_detail: env_detail.as_ref(),
        base_vars: &base_vars,
        base_secrets: &base_secrets,
        target_base_url: target_base_url.as_deref(),
        target_tls: &target_tls,
        target_tenants: &target_tenants,
    };
    let (kind, src_req_id, src_col_id, target_id, title, exec_requests) =
        compile_source_requests(conn, create, &env_ctx)?;

    Ok(RunSnapshot {
        title: title.clone(),
        source: RunSourceMeta {
            kind,
            request_id: src_req_id,
            collection_id: src_col_id,
            environment_id: env_detail.as_ref().map(|e| e.id.clone()),
            environment_name: env_detail.as_ref().map(|e| e.name.clone()),
            load_profile_id: profile_id,
            load_profile_name: profile_name,
        },
        target: ExecutionTarget {
            id: target_id,
            name: title,
            environment_id: env_detail.as_ref().map(|e| e.id.clone()),
            environment_name: env_detail.as_ref().map(|e| e.name.clone()),
            gateway_base_url: target_base_url,
            tls: target_tls,
            tenants: target_tenants,
            requests: exec_requests,
        },
        template: load_config,
        assert,
    })
}

fn compile_source_requests(
    conn: &Connection,
    create: &CreateRunRequest,
    env_ctx: &CompileEnvCtx<'_>,
) -> Result<CompiledSource, StoreError> {
    let has_req_id = non_empty_str(create.request_id.as_deref()).is_some();
    let has_col_id = non_empty_str(create.collection_id.as_deref()).is_some();
    let has_adhoc_one = create.adhoc_request.is_some();
    let has_adhoc_many = create
        .adhoc_requests
        .as_ref()
        .is_some_and(|list| !list.is_empty());

    let source_modes = usize::from(has_req_id)
        + usize::from(has_col_id)
        + usize::from(has_adhoc_one || has_adhoc_many);
    if source_modes == 0 {
        return Err(StoreError::invalid(
            "provide one of request_id, collection_id, or adhoc_request(s) to start a run",
        ));
    }
    if source_modes > 1 && !(has_adhoc_one && has_col_id && !has_req_id) {
        return Err(StoreError::invalid(
            "request_id, collection_id, and adhoc_request are mutually exclusive",
        ));
    }

    if has_adhoc_one || has_adhoc_many {
        compile_adhoc_source(conn, create, env_ctx)
    } else if let Some(rid) = non_empty_str(create.request_id.as_deref()) {
        let saved = get_request(conn, rid)?;
        let chain = ancestor_chain(conn, &saved.collection_id)?;
        let norm = saved.to_draft().validate(true)?;
        let req = build_execution_request(
            saved.id.clone(),
            &norm,
            &chain,
            env_ctx.env_detail.map(|e| &e.http_auth),
            env_ctx.base_vars,
            env_ctx.base_secrets,
            create.variables_override.as_ref(),
        );
        validate_compiled_request(
            &req,
            env_ctx.target_base_url,
            env_ctx.target_tls,
            env_ctx.target_tenants,
        )?;
        Ok((
            "request".to_string(),
            Some(saved.id.clone()),
            Some(saved.collection_id),
            saved.id,
            req.name.clone(),
            vec![req],
        ))
    } else {
        compile_collection_source(conn, create, env_ctx)
    }
}

fn compile_adhoc_source(
    conn: &Connection,
    create: &CreateRunRequest,
    env_ctx: &CompileEnvCtx<'_>,
) -> Result<CompiledSource, StoreError> {
    let drafts: Vec<RequestDraft> =
        if let Some(list) = create.adhoc_requests.as_ref().filter(|l| !l.is_empty()) {
            list.clone()
        } else if let Some(single) = create.adhoc_request.as_ref() {
            vec![single.clone()]
        } else {
            Vec::new()
        };
    let hint_col_id = non_empty_owned(create.collection_id.as_deref());
    let mut built = Vec::with_capacity(drafts.len());
    for draft in &drafts {
        let cid = draft
            .collection_id
            .as_deref()
            .and_then(|s| non_empty_owned(Some(s)))
            .or_else(|| hint_col_id.clone());
        let chain = match cid.as_deref() {
            Some(c) => ancestor_chain(conn, c)?,
            None => Vec::new(),
        };
        let norm = draft.validate(false)?;
        let id = draft
            .id
            .as_deref()
            .and_then(|s| non_empty_owned(Some(s)))
            .unwrap_or_else(new_id);
        let req = build_execution_request(
            id,
            &norm,
            &chain,
            env_ctx.env_detail.map(|e| &e.http_auth),
            env_ctx.base_vars,
            env_ctx.base_secrets,
            create.variables_override.as_ref(),
        );
        validate_compiled_request(
            &req,
            env_ctx.target_base_url,
            env_ctx.target_tls,
            env_ctx.target_tenants,
        )?;
        built.push(req);
    }
    let run_title = if built.len() == 1 {
        built[0].name.clone()
    } else {
        format!("Ad-hoc Batch ({} requests)", built.len())
    };
    Ok((
        "adhoc".to_string(),
        None,
        hint_col_id,
        "adhoc".to_string(),
        run_title,
        built,
    ))
}

fn compile_collection_source(
    conn: &Connection,
    create: &CreateRunRequest,
    env_ctx: &CompileEnvCtx<'_>,
) -> Result<CompiledSource, StoreError> {
    let cid = non_empty_str(create.collection_id.as_deref()).unwrap_or_default();
    let subtree = get_collection_subtree(conn, cid)?;
    let all_saved = flatten_subtree_requests(&subtree);
    if all_saved.is_empty() {
        return Err(StoreError::invalid(format!(
            "collection '{}' contains no requests to run",
            subtree.name
        )));
    }
    let selected = select_collection_requests(&all_saved, create.selected_requests.as_deref())?;
    let mut chain_cache: HashMap<String, Vec<CollectionRecord>> = HashMap::new();
    let mut built = Vec::with_capacity(selected.len());
    for (saved, weight) in selected {
        let chain = if let Some(cached) = chain_cache.get(&saved.collection_id) {
            cached.clone()
        } else {
            let loaded = ancestor_chain(conn, &saved.collection_id)?;
            chain_cache.insert(saved.collection_id.clone(), loaded.clone());
            loaded
        };
        let mut draft = saved.to_draft();
        draft.weight = weight;
        let norm = draft.validate(true)?;
        let req = build_execution_request(
            saved.id.clone(),
            &norm,
            &chain,
            env_ctx.env_detail.map(|e| &e.http_auth),
            env_ctx.base_vars,
            env_ctx.base_secrets,
            create.variables_override.as_ref(),
        );
        validate_compiled_request(
            &req,
            env_ctx.target_base_url,
            env_ctx.target_tls,
            env_ctx.target_tenants,
        )?;
        built.push(req);
    }
    Ok((
        "collection".to_string(),
        None,
        Some(subtree.id.clone()),
        subtree.id,
        subtree.name,
        built,
    ))
}

fn base_secret_var_keys(
    global: &EnvironmentDetail,
    selected: Option<&EnvironmentDetail>,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for v in &global.variables {
        if v.active && v.secret && !v.key.trim().is_empty() {
            keys.insert(v.key.trim().to_string());
        }
    }
    if let Some(env) = selected {
        for v in &env.variables {
            if v.active && !v.key.trim().is_empty() {
                if v.secret {
                    keys.insert(v.key.trim().to_string());
                } else {
                    keys.remove(v.key.trim());
                }
            }
        }
    }
    keys
}

fn select_collection_requests<'a>(
    all: &'a [SavedRequest],
    selection: Option<&[crate::db::runs::SelectedRequestWeight]>,
) -> Result<Vec<(&'a SavedRequest, i64)>, StoreError> {
    let Some(items) = selection.filter(|s| !s.is_empty()) else {
        return Ok(all.iter().map(|r| (r, r.weight.max(1))).collect());
    };
    let by_id: HashMap<&str, &SavedRequest> = all.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut out = Vec::with_capacity(items.len());
    for sel in items {
        if sel.weight <= 0 {
            return Err(StoreError::invalid("selected request weight must be > 0"));
        }
        let req = by_id.get(sel.request_id.trim()).copied().ok_or_else(|| {
            StoreError::invalid(format!(
                "selected request '{}' does not belong to the chosen collection",
                sel.request_id
            ))
        })?;
        out.push((req, sel.weight));
    }
    Ok(out)
}

fn build_execution_request(
    id: String,
    norm: &NormalizedRequest,
    chain: &[CollectionRecord],
    env_auth: Option<&HttpAuthSpec>,
    base_vars: &BTreeMap<String, String>,
    base_secrets: &BTreeSet<String>,
    vars_override: Option<&BTreeMap<String, String>>,
) -> ExecutionRequest {
    let mut variables = base_vars.clone();
    let mut secret_keys = base_secrets.clone();
    for (k, v) in merge_collection_variables(chain) {
        variables.insert(k, v);
    }
    for col in chain {
        for v in &col.variables {
            if v.active && !v.key.trim().is_empty() {
                if v.secret {
                    secret_keys.insert(v.key.trim().to_string());
                } else {
                    secret_keys.remove(v.key.trim());
                }
            }
        }
    }
    if let Some(overrides) = vars_override {
        for (k, v) in overrides {
            if !k.trim().is_empty() {
                variables.insert(k.trim().to_string(), v.clone());
            }
        }
    }
    let resolved_auth = resolve_inherited_auth(env_auth, chain, &norm.auth);
    let merged_headers = merge_inherited_headers(chain, &norm.headers);

    ExecutionRequest {
        id,
        name: norm.name.clone(),
        protocol: norm.protocol.clone(),
        method: norm.method.clone(),
        endpoint: norm.endpoint.clone(),
        params: norm.params.clone(),
        headers: merged_headers,
        body: norm.body.clone(),
        auth: resolved_auth,
        mazbri: norm.mazbri.clone(),
        tls: norm.tls.clone(),
        assertions: norm.assertions.clone(),
        weight: norm.weight,
        timeout_secs: norm.timeout_secs,
        variables,
        secret_variables: secret_keys.into_iter().collect(),
    }
}

/// Validate that a compiled request has a reachable `http(s)://` URL (for `http` or `mazbri`)
/// and valid Mazbri tenant credentials (when `protocol == "mazbri"`).
///
/// # Errors
///
/// Returns validation error when URL, TLS, or Mazbri tenant settings are invalid.
pub fn validate_compiled_request(
    req: &ExecutionRequest,
    env_base_url: Option<&str>,
    env_tls: &TlsSpec,
    env_tenants: &[TenantSpec],
) -> Result<(), StoreError> {
    let merged_tls = req.tls.overlay_on(env_tls);
    merged_tls.validate()?;

    let sample_runtime = RuntimeVars::for_index(0);
    if req.protocol == PROTOCOL_HTTP {
        let rendered_ep = render_str(&req.endpoint, &req.variables, &sample_runtime);
        let full_url = if rendered_ep.starts_with("http://") || rendered_ep.starts_with("https://")
        {
            rendered_ep
        } else {
            let base_raw = env_base_url
                .and_then(|s| non_empty_str(Some(s)))
                .or_else(|| req.variables.get("baseUrl").map(String::as_str))
                .ok_or_else(|| {
                    StoreError::invalid(format!(
                        "HTTP request '{}' uses a relative path {:?}, so an Environment gateway_base_url or <<baseUrl>> variable is required",
                        req.name, req.endpoint
                    ))
                })?;
            let rendered_base = render_str(base_raw, &req.variables, &sample_runtime);
            join_base_and_path(&rendered_base, &rendered_ep)
        };
        validate_http_url(&full_url, &format!("request '{}' URL", req.name))?;
    } else if req.protocol == PROTOCOL_MAZBRI {
        let raw_gw = non_empty_str(req.mazbri.gateway_base_url.as_deref())
            .or_else(|| non_empty_str(env_base_url))
            .or_else(|| req.variables.get("gateway_base_url").map(String::as_str))
            .or_else(|| req.variables.get("baseUrl").map(String::as_str))
            .ok_or_else(|| {
                StoreError::invalid(format!(
                    "Mazbri request '{}' requires gateway_base_url (set on Environment or request)",
                    req.name
                ))
            })?;
        let rendered_gw = render_str(raw_gw, &req.variables, &sample_runtime);
        validate_http_url(
            &rendered_gw,
            &format!("Mazbri request '{}' gateway_base_url", req.name),
        )?;

        match req.mazbri.tenant_source.trim() {
            "inline" => {
                let inline = req.mazbri.inline_tenant.as_ref().ok_or_else(|| {
                    StoreError::invalid(format!(
                        "Mazbri request '{}' has tenant_source='inline' without inline_tenant",
                        req.name
                    ))
                })?;
                inline.validate_for_mazbri()?;
            }
            "environment" => {
                if env_tenants.is_empty() {
                    return Err(StoreError::invalid(format!(
                        "Mazbri request '{}' uses environment tenant pool, but the selected Environment has no tenants",
                        req.name
                    )));
                }
                if let Some(pinned) = non_empty_str(req.mazbri.tenant_name.as_deref()) {
                    let found = env_tenants
                        .iter()
                        .find(|t| t.name.trim() == pinned)
                        .ok_or_else(|| {
                            StoreError::invalid(format!(
                                "Mazbri request '{}' pins tenant '{pinned}', which was not found in the selected Environment",
                                req.name
                            ))
                        })?;
                    found.validate_for_mazbri()?;
                } else {
                    for tenant in env_tenants {
                        tenant.validate_for_mazbri()?;
                    }
                }
            }
            other => {
                return Err(StoreError::invalid(format!(
                    "unsupported mazbri.tenant_source {other:?}"
                )));
            }
        }
    }
    Ok(())
}

fn join_base_and_path(base: &str, path: &str) -> String {
    let trimmed_base = base.trim().trim_end_matches('/');
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        trimmed_base.to_string()
    } else if trimmed_path.starts_with('/') {
        format!("{trimmed_base}{trimmed_path}")
    } else {
        format!("{trimmed_base}/{trimmed_path}")
    }
}
