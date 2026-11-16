import { http, type RequestOptions } from "./http";
import type {
  CollectionInput,
  CollectionNode,
  CreateRunRequest,
  EnvironmentDetail,
  EnvironmentInput,
  EnvironmentSummary,
  GeneratedBindingKeypair,
  LoadProfile,
  LoadProfileInput,
  MeResponse,
  MetaResponse,
  ProbeRecord,
  ProbeRequest,
  ProbeResponse,
  ProbesPage,
  ProbesQuery,
  ReportPayload,
  RequestDraft,
  RunStats,
  RunView,
  SampleCursor,
  SampleFilters,
  SampleRow,
  SavedRequest,
  TenantSpec,
  TimePoint,
  TimelineEvent,
  Verdict,
  WindowRow,
} from "./types";

export const api = {
  // Auth & Meta
  me: () => http.get<MeResponse>("/api/v1/auth/me").then((r) => r.data),
  login: (token: string) =>
    http
      .post<{ must_change_password: boolean }>("/api/v1/auth/login", { token })
      .then((r) => r.data),
  logout: () => http.post<void>("/api/v1/auth/logout").then((r) => r.data),
  changePassword: (old_token: string, new_token: string) =>
    http
      .post<void>("/api/v1/auth/password", { old_token, new_token })
      .then((r) => r.data),
  meta: (opts?: RequestOptions) =>
    http.get<MetaResponse>("/api/v1/meta", { ...opts }).then((r) => r.data),

  // Collections & Requests
  listCollections: () =>
    http.get<CollectionNode[]>("/api/v1/collections").then((r) => r.data),
  getCollection: (id: string) =>
    http.get<CollectionNode>(`/api/v1/collections/${id}`).then((r) => r.data),
  createCollection: (input: CollectionInput) =>
    http
      .post<CollectionNode>("/api/v1/collections", input)
      .then((r) => r.data),
  updateCollection: (id: string, input: CollectionInput) =>
    http
      .put<CollectionNode>(`/api/v1/collections/${id}`, input)
      .then((r) => r.data),
  deleteCollection: (id: string) =>
    http.delete<void>(`/api/v1/collections/${id}`).then((r) => r.data),
  exportCollections: (params?: {
    collection_id?: string;
    include_secrets?: boolean;
  }) =>
    http
      .get<unknown>("/api/v1/collections/export", { params })
      .then((r) => r.data),
  importCollections: (payload: unknown, parent_id?: string) =>
    http
      .post<CollectionNode[]>("/api/v1/collections/import", payload, {
        params: parent_id ? { parent_id } : undefined,
      })
      .then((r) => r.data),

  getRequest: (id: string) =>
    http.get<SavedRequest>(`/api/v1/requests/${id}`).then((r) => r.data),
  createRequest: (draft: RequestDraft) =>
    http.post<SavedRequest>("/api/v1/requests", draft).then((r) => r.data),
  updateRequest: (id: string, draft: RequestDraft) =>
    http
      .put<SavedRequest>(`/api/v1/requests/${id}`, draft)
      .then((r) => r.data),
  deleteRequest: (id: string) =>
    http.delete<void>(`/api/v1/requests/${id}`).then((r) => r.data),

  // Environments & Tenants
  listEnvironments: () =>
    http
      .get<EnvironmentSummary[]>("/api/v1/environments")
      .then((r) => r.data),
  getEnvironment: (id: string) =>
    http
      .get<EnvironmentDetail>(`/api/v1/environments/${id}`)
      .then((r) => r.data),
  createEnvironment: (input: EnvironmentInput) =>
    http
      .post<EnvironmentDetail>("/api/v1/environments", input)
      .then((r) => r.data),
  updateEnvironment: (id: string, input: EnvironmentInput) =>
    http
      .put<EnvironmentDetail>(`/api/v1/environments/${id}`, input)
      .then((r) => r.data),
  deleteEnvironment: (id: string) =>
    http.delete<void>(`/api/v1/environments/${id}`).then((r) => r.data),
  exportEnvironments: (params?: {
    environment_id?: string;
    include_secrets?: boolean;
  }) =>
    http
      .get<unknown>("/api/v1/environments/export", { params })
      .then((r) => r.data),
  importEnvironments: (payload: unknown) =>
    http
      .post<EnvironmentDetail[]>("/api/v1/environments/import", payload)
      .then((r) => r.data),

  listTenants: (envId: string) =>
    http
      .get<TenantSpec[]>(`/api/v1/environments/${envId}/tenants`)
      .then((r) => r.data),
  createTenant: (envId: string, tenant: TenantSpec) =>
    http
      .post<TenantSpec>(`/api/v1/environments/${envId}/tenants`, tenant)
      .then((r) => r.data),
  updateTenant: (envId: string, tid: string, tenant: TenantSpec) =>
    http
      .put<TenantSpec>(`/api/v1/environments/${envId}/tenants/${tid}`, tenant)
      .then((r) => r.data),
  deleteTenant: (envId: string, tid: string) =>
    http
      .delete<void>(`/api/v1/environments/${envId}/tenants/${tid}`)
      .then((r) => r.data),
  generateTenantKeypair: (
    envId: string,
    tid: string,
    alg: string,
    kid?: string,
  ) =>
    http
      .post<GeneratedBindingKeypair>(
        `/api/v1/environments/${envId}/tenants/${tid}/binding-keypair`,
        { alg, kid },
      )
      .then((r) => r.data),

  // Stateless Crypto & Interactive Probe
  generateStatelessKeypair: (alg: string, kid: string) =>
    http
      .post<GeneratedBindingKeypair>("/api/v1/crypto/binding-keypair", {
        alg,
        kid,
      })
      .then((r) => r.data),
  probe: (req: ProbeRequest) =>
    http.post<ProbeResponse>("/api/v1/probe", req).then((r) => r.data),
  listProbes: (params?: ProbesQuery) =>
    http.get<ProbesPage>("/api/v1/probes", { params }).then((r) => r.data),
  getProbe: (id: number) =>
    http.get<ProbeRecord>(`/api/v1/probes/${id}`).then((r) => r.data),
  deleteProbe: (id: number) =>
    http.delete<void>(`/api/v1/probes/${id}`).then((r) => r.data),

  // Load Profiles
  listLoadProfiles: () =>
    http.get<LoadProfile[]>("/api/v1/load-profiles").then((r) => r.data),
  getLoadProfile: (id: string) =>
    http.get<LoadProfile>(`/api/v1/load-profiles/${id}`).then((r) => r.data),
  createLoadProfile: (input: LoadProfileInput) =>
    http
      .post<LoadProfile>("/api/v1/load-profiles", input)
      .then((r) => r.data),
  updateLoadProfile: (id: string, input: LoadProfileInput) =>
    http
      .put<LoadProfile>(`/api/v1/load-profiles/${id}`, input)
      .then((r) => r.data),
  deleteLoadProfile: (id: string) =>
    http.delete<void>(`/api/v1/load-profiles/${id}`).then((r) => r.data),

  // Runs
  listRuns: (status?: string, opts?: RequestOptions) =>
    http
      .get<RunView[]>("/api/v1/runs", {
        params: status ? { status } : undefined,
        ...opts,
      })
      .then((r) => r.data),
  getRun: (id: string, opts?: RequestOptions) =>
    http.get<RunView>(`/api/v1/runs/${id}`, { ...opts }).then((r) => r.data),
  createRun: (req: CreateRunRequest) =>
    http.post<RunView>("/api/v1/runs", req).then((r) => r.data),
  deleteRun: (id: string) =>
    http.delete<void>(`/api/v1/runs/${id}`).then((r) => r.data),
  stopRun: (id: string) =>
    http.post<RunView>(`/api/v1/runs/${id}/stop`).then((r) => r.data),
  resumeRun: (id: string) =>
    http.post<RunView>(`/api/v1/runs/${id}/resume`).then((r) => r.data),
  patchTraffic: (
    id: string,
    patch: { qps?: number | null; concurrency?: number | null },
  ) =>
    http
      .patch<RunView>(`/api/v1/runs/${id}/traffic`, patch)
      .then((r) => r.data),
  getRunStats: (id: string, opts?: RequestOptions) =>
    http
      .get<RunStats>(`/api/v1/runs/${id}/stats`, { ...opts })
      .then((r) => r.data),
  getRunTimeseries: (id: string, opts?: RequestOptions) =>
    http
      .get<{ points?: TimePoint[] } | TimePoint[]>(
        `/api/v1/runs/${id}/timeseries`,
        { ...opts },
      )
      .then((r) => (Array.isArray(r.data) ? r.data : (r.data.points ?? []))),
  getRunTimeline: (id: string, opts?: RequestOptions) =>
    http
      .get<{ events?: TimelineEvent[] } | TimelineEvent[]>(
        `/api/v1/runs/${id}/timeline`,
        { ...opts },
      )
      .then((r) => (Array.isArray(r.data) ? r.data : (r.data.events ?? []))),
  getRunWindows: (id: string, opts?: RequestOptions) =>
    http
      .get<{ windows?: WindowRow[] } | WindowRow[]>(
        `/api/v1/runs/${id}/windows`,
        { ...opts },
      )
      .then((r) => (Array.isArray(r.data) ? r.data : (r.data.windows ?? []))),
  getRunVerdict: (id: string) =>
    http.get<Verdict>(`/api/v1/runs/${id}/verdict`).then((r) => r.data),
  getRunSamples: (
    id: string,
    params: SampleFilters & Partial<SampleCursor> & { limit?: number },
    opts?: RequestOptions,
  ) =>
    http
      .get<{ items?: SampleRow[]; next?: SampleCursor | null }>(
        `/api/v1/runs/${id}/samples`,
        { params, ...opts },
      )
      .then((r) => ({
        items: r.data.items ?? [],
        next: r.data.next ?? null,
      })),
  getRunReport: (id: string, opts?: RequestOptions) =>
    http
      .get<ReportPayload>(`/api/v1/runs/${id}/report`, { ...opts })
      .then((r) => r.data),
  downloadReportBlob: (id: string, kind: "json" | "html") =>
    http
      .get<Blob>(`/api/v1/runs/${id}/report.${kind}`, { responseType: "blob" })
      .then((r) => r.data),
};
