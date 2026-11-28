import { computed, markRaw, ref, watch } from "vue";
import { api } from "../api/workspace";
import type {
  AssertSpec,
  CollectionNode,
  EnvironmentDetail,
  EnvironmentSummary,
  LoadConfig,
  LoadProfile,
  MetaResponse,
  ProbeRecord,
  ProbeResponse,
  RequestDraft,
  RunReplayPayload,
  SavedRequest,
} from "../api/types";
import {
  cloneJson,
  createDefaultRequestDraft,
  normalizeDraftForApi,
} from "../helpers/format";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";

export interface RerunLoadConfig {
  fromRunId: string;
  fromTitle: string;
  environmentId: string | null;
  loadConfig: LoadConfig;
  assertSpec: AssertSpec | null;
}

export interface WorkspaceTab {
  tabId: string;
  draft: RequestDraft;
  savedSnapshotJson: string | null;
  baselineSnapshotJson: string;
  requestSubTab:
    | "params"
    | "body"
    | "headers"
    | "auth"
    | "assertions"
    | "captures"
    | "tls";
  responseSubTab:
    | "json"
    | "raw"
    | "headers"
    | "assertions"
    | "preview"
    | "mazbri_wire";
  sending: boolean;
  response: ProbeResponse | null;
  /** Per-protocol endpoint/method/body kept while the other protocol is active. */
  protocolStash: ProtocolStash;
  /** Attached when opened via "重跑" from a historical load test. */
  rerunConfig?: RerunLoadConfig | null;
}

export interface ProtocolFields {
  endpoint: string;
  method: RequestDraft["method"];
  body: RequestDraft["body"];
}

export type ProtocolStash = Partial<Record<"http" | "mazbri", ProtocolFields>>;

export type RightSidebarSection =
  | "collections"
  | "environments"
  | "profiles"
  | "history";

export interface LoadRunnerLaunchContext {
  mode: "single" | "collection" | "batch";
  draft?: RequestDraft;
  batchDrafts?: RequestDraft[];
  collectionId?: string;
  collectionName?: string;
  initialEnvId?: string | null;
  initialLoadConfig?: LoadConfig | null;
  initialAssertSpec?: AssertSpec | null;
  initialTitle?: string | null;
  fromRunId?: string | null;
}

const TABS_STORAGE_KEY = "x-loadgen.workspace.tabs.v3";
const ACTIVE_TAB_STORAGE_KEY = "x-loadgen.workspace.activeTab.v2";
const ENV_STORAGE_KEY = "x-loadgen.workspace.envId.v2";
const LAYOUT_STORAGE_KEY = "x-loadgen.workspace.layout.v3";
/**
 * Draft secrets live in sessionStorage (per browser tab, gone when it closes)
 * keyed by tabId; localStorage only ever holds the redacted drafts.
 */
const SECRETS_STORAGE_KEY = "x-loadgen.workspace.secrets.v1";
const PERSIST_DEBOUNCE_MS = 300;

/** Credential fields of a draft that must never reach localStorage. */
const SECRET_PATHS: readonly (readonly string[])[] = [
  ["auth", "token"],
  ["auth", "password"],
  ["auth", "value"],
  ["tls", "client_key"],
  ["mazbri", "inline_tenant", "api_key"],
  ["mazbri", "inline_tenant", "tenant_key_base64"],
];

type DraftSecrets = Record<string, string>;

/** Secrets removed from one persisted tab: its draft and both dirty-check snapshots. */
interface TabSecrets {
  draft?: DraftSecrets;
  saved?: DraftSecrets;
  baseline?: DraftSecrets;
}

function readPath(obj: unknown, path: readonly string[]): unknown {
  let cur: unknown = obj;
  for (const key of path) {
    if (cur === null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[key];
  }
  return cur;
}

function writePath(obj: unknown, path: readonly string[], value: unknown): void {
  let cur: unknown = obj;
  for (const key of path.slice(0, -1)) {
    if (cur === null || typeof cur !== "object") return;
    cur = (cur as Record<string, unknown>)[key];
  }
  if (cur !== null && typeof cur === "object") {
    (cur as Record<string, unknown>)[path[path.length - 1]!] = value;
  }
}

/** Copy of `draft` without credentials, plus the removed values. */
function splitDraftSecrets(draft: RequestDraft): {
  redacted: RequestDraft;
  secrets: DraftSecrets;
} {
  const redacted = cloneJson(draft);
  const secrets: DraftSecrets = {};
  for (const path of SECRET_PATHS) {
    const value = readPath(redacted, path);
    if (typeof value === "string" && value !== "") {
      secrets[path.join(".")] = value;
      writePath(redacted, path, "");
    }
  }
  return { redacted, secrets };
}

function mergeDraftSecrets(draft: RequestDraft, secrets: DraftSecrets | undefined): void {
  if (!secrets) return;
  for (const path of SECRET_PATHS) {
    const value = secrets[path.join(".")];
    if (value !== undefined) writePath(draft, path, value);
  }
}

/** Redact a JSON snapshot string; returns it unchanged if it doesn't parse. */
function splitSnapshotSecrets(json: string): { json: string; secrets?: DraftSecrets } {
  try {
    const { redacted, secrets } = splitDraftSecrets(JSON.parse(json) as RequestDraft);
    return Object.keys(secrets).length > 0
      ? { json: JSON.stringify(redacted), secrets }
      : { json };
  } catch {
    return { json };
  }
}

function mergeSnapshotSecrets(json: string, secrets: DraftSecrets | undefined): string {
  if (!secrets) return json;
  try {
    const draft = JSON.parse(json) as RequestDraft;
    mergeDraftSecrets(draft, secrets);
    return JSON.stringify(draft);
  } catch {
    return json;
  }
}

function loadSessionSecrets(): Record<string, TabSecrets> {
  try {
    const raw = sessionStorage.getItem(SECRETS_STORAGE_KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : null;
    return parsed && typeof parsed === "object"
      ? (parsed as Record<string, TabSecrets>)
      : {};
  } catch {
    return {};
  }
}

function makeTabId(): string {
  return `tab-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

export function createWorkspaceTab(
  draft?: RequestDraft,
  isSaved = false,
): WorkspaceTab {
  const initial = draft ? cloneJson(draft) : createDefaultRequestDraft("http");
  const snapshot = JSON.stringify(normalizeDraftForApi(initial));
  return {
    tabId: makeTabId(),
    draft: initial,
    savedSnapshotJson: isSaved ? snapshot : null,
    baselineSnapshotJson: snapshot,
    requestSubTab: initial.protocol === "mazbri" ? "auth" : "params",
    responseSubTab: "json",
    sending: false,
    response: null,
    protocolStash: {},
  };
}

function loadInitialTabs(): WorkspaceTab[] {
  try {
    const raw = localStorage.getItem(TABS_STORAGE_KEY);
    if (!raw) return [createWorkspaceTab()];
    const parsed = JSON.parse(raw) as Partial<WorkspaceTab>[];
    if (!Array.isArray(parsed) || parsed.length === 0) {
      return [createWorkspaceTab()];
    }
    const secrets = loadSessionSecrets();
    return parsed.map((item) => {
      const draft = item.draft ?? createDefaultRequestDraft("http");
      const tabSecrets = item.tabId ? secrets[item.tabId] : undefined;
      mergeDraftSecrets(draft, tabSecrets?.draft);
      const defaultBaseline = JSON.stringify(
        normalizeDraftForApi(
          createDefaultRequestDraft(
            draft.protocol === "mazbri" ? "mazbri" : "http",
          ),
        ),
      );
      return {
        tabId: item.tabId || makeTabId(),
        draft,
        savedSnapshotJson: item.savedSnapshotJson
          ? mergeSnapshotSecrets(item.savedSnapshotJson, tabSecrets?.saved)
          : null,
        baselineSnapshotJson: item.baselineSnapshotJson
          ? mergeSnapshotSecrets(item.baselineSnapshotJson, tabSecrets?.baseline)
          : defaultBaseline,
        requestSubTab: item.requestSubTab ?? "params",
        responseSubTab: item.responseSubTab ?? "json",
        sending: false,
        response: null,
        protocolStash: item.protocolStash ?? {},
      };
    });
  } catch {
    return [createWorkspaceTab()];
  }
}

interface LayoutPrefs {
  leftExpanded: boolean;
  rightOpen: boolean;
  rightSection: RightSidebarSection;
  horizontalSplit: boolean;
}

function loadLayoutPrefs(): LayoutPrefs {
  try {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (!raw) {
      return {
        leftExpanded: true,
        rightOpen: true,
        rightSection: "collections",
        horizontalSplit: false,
      };
    }
    const p = JSON.parse(raw) as Partial<LayoutPrefs>;
    const validSection: RightSidebarSection =
      p.rightSection === "environments" ||
      p.rightSection === "profiles" ||
      p.rightSection === "history"
        ? p.rightSection
        : "collections";
    return {
      leftExpanded: p.leftExpanded !== false,
      rightOpen: p.rightOpen !== false,
      rightSection: validSection,
      horizontalSplit: Boolean(p.horizontalSplit),
    };
  } catch {
    return {
      leftExpanded: true,
      rightOpen: true,
      rightSection: "collections",
      horizontalSplit: false,
    };
  }
}

const initialTabs = loadInitialTabs();
const initialLayout = loadLayoutPrefs();

export const tabs = ref<WorkspaceTab[]>(initialTabs);
export const activeTabId = ref<string>(
  localStorage.getItem(ACTIVE_TAB_STORAGE_KEY) &&
    initialTabs.some(
      (t) => t.tabId === localStorage.getItem(ACTIVE_TAB_STORAGE_KEY),
    )
    ? (localStorage.getItem(ACTIVE_TAB_STORAGE_KEY) as string)
    : initialTabs[0]!.tabId,
);

export const activeTab = computed<WorkspaceTab>(() => {
  return (
    tabs.value.find((t) => t.tabId === activeTabId.value) ?? tabs.value[0]!
  );
});

function computeTabDirty(tab: WorkspaceTab): boolean {
  const current = JSON.stringify(normalizeDraftForApi(tab.draft));
  if (tab.savedSnapshotJson) {
    return current !== tab.savedSnapshotJson;
  }
  return current !== tab.baselineSnapshotJson;
}

const dirtyTabIds = computed<ReadonlySet<string>>(() => {
  const dirty = new Set<string>();
  for (const tab of tabs.value) {
    if (computeTabDirty(tab)) dirty.add(tab.tabId);
  }
  return dirty;
});

export function isTabDirty(tab: WorkspaceTab): boolean {
  if (tabs.value.some((t) => t.tabId === tab.tabId)) {
    return dirtyTabIds.value.has(tab.tabId);
  }
  return computeTabDirty(tab);
}

// Layout state (responsePaneOpen defaults to false so bottom response bar is hidden initially)
export const leftSidebarExpanded = ref<boolean>(initialLayout.leftExpanded);
export const rightSidebarOpen = ref<boolean>(initialLayout.rightOpen);
export const rightSidebarSection = ref<RightSidebarSection>(
  initialLayout.rightSection,
);
export const horizontalSplit = ref<boolean>(initialLayout.horizontalSplit);
export const responsePaneOpen = ref<boolean>(false);
/** Bumped after every Send so the probe history reloads. */
export const probeHistoryVersion = ref(0);

watch(
  [leftSidebarExpanded, rightSidebarOpen, rightSidebarSection, horizontalSplit],
  () => {
    localStorage.setItem(
      LAYOUT_STORAGE_KEY,
      JSON.stringify({
        leftExpanded: leftSidebarExpanded.value,
        rightOpen: rightSidebarOpen.value,
        rightSection: rightSidebarSection.value,
        horizontalSplit: horizontalSplit.value,
      }),
    );
  },
);

function persistTabs(): void {
  try {
    const secrets: Record<string, TabSecrets> = {};
    const compact = tabs.value.map((t) => {
      const draft = splitDraftSecrets(t.draft);
      const saved = t.savedSnapshotJson
        ? splitSnapshotSecrets(t.savedSnapshotJson)
        : null;
      const baseline = splitSnapshotSecrets(t.baselineSnapshotJson);
      const tabSecrets: TabSecrets = {};
      if (Object.keys(draft.secrets).length > 0) tabSecrets.draft = draft.secrets;
      if (saved?.secrets) tabSecrets.saved = saved.secrets;
      if (baseline.secrets) tabSecrets.baseline = baseline.secrets;
      if (Object.keys(tabSecrets).length > 0) secrets[t.tabId] = tabSecrets;
      return {
        tabId: t.tabId,
        draft: draft.redacted,
        savedSnapshotJson: saved?.json ?? null,
        baselineSnapshotJson: baseline.json,
        requestSubTab: t.requestSubTab,
        responseSubTab: t.responseSubTab,
        protocolStash: t.protocolStash,
      };
    });
    localStorage.setItem(TABS_STORAGE_KEY, JSON.stringify(compact));
    sessionStorage.setItem(SECRETS_STORAGE_KEY, JSON.stringify(secrets));
  } catch {
    // Ignore storage quota errors
  }
}

let persistTimer: number | null = null;

function flushPersistTabs(): void {
  if (persistTimer !== null) {
    window.clearTimeout(persistTimer);
    persistTimer = null;
  }
  persistTabs();
}

watch(
  () =>
    tabs.value.map((t) => [
      t.tabId,
      t.draft,
      t.savedSnapshotJson,
      t.baselineSnapshotJson,
      t.requestSubTab,
      t.responseSubTab,
      t.protocolStash,
    ]),
  () => {
    if (persistTimer !== null) window.clearTimeout(persistTimer);
    persistTimer = window.setTimeout(flushPersistTabs, PERSIST_DEBOUNCE_MS);
  },
  { deep: true },
);
window.addEventListener("pagehide", flushPersistTabs);
// Rewrite storage right away so nothing loaded from it stays unredacted.
persistTabs();

/**
 * Forget all locally kept drafts and their secrets (logout on a shared
 * machine must not leave them behind) and start with one blank tab.
 */
export function clearWorkspaceSession(): void {
  if (persistTimer !== null) {
    window.clearTimeout(persistTimer);
    persistTimer = null;
  }
  const fresh = createWorkspaceTab();
  tabs.value = [fresh];
  activeTabId.value = fresh.tabId;
  localStorage.removeItem(TABS_STORAGE_KEY);
  localStorage.removeItem(ACTIVE_TAB_STORAGE_KEY);
  sessionStorage.removeItem(SECRETS_STORAGE_KEY);
}

watch(activeTabId, (id) => {
  localStorage.setItem(ACTIVE_TAB_STORAGE_KEY, id);
});

// Server-backed workspace catalog
export const collections = ref<CollectionNode[]>([]);
export const environments = ref<EnvironmentSummary[]>([]);
export const activeEnvironmentId = ref<string | null>(
  localStorage.getItem(ENV_STORAGE_KEY) || null,
);
export const activeEnvironmentDetail = ref<EnvironmentDetail | null>(null);
export const globalEnvironmentDetail = ref<EnvironmentDetail | null>(null);
export const loadProfiles = ref<LoadProfile[]>([]);
export const serverMeta = ref<MetaResponse | null>(null);
export const workspaceLoaded = ref<boolean>(false);

// Modals
export const envModalOpen = ref<boolean>(false);
export const editingEnvId = ref<string | null>(null);
export const saveModalOpen = ref<boolean>(false);
export const saveModalForceNew = ref<boolean>(false);
export const curlModalOpen = ref<boolean>(false);
export const runnerModalOpen = ref<boolean>(false);
export const runnerContext = ref<LoadRunnerLaunchContext>({
  mode: "single",
});

export const selectableEnvironments = computed<EnvironmentSummary[]>(() =>
  environments.value.filter((e) => !e.is_global),
);

export const effectiveVariables = computed<
  { key: string; value: string; secret: boolean; scope: string }[]
>(() => {
  const map = new Map<
    string,
    { key: string; value: string; secret: boolean; scope: string }
  >();
  for (const v of globalEnvironmentDetail.value?.variables ?? []) {
    if (v.active && v.key.trim()) {
      map.set(v.key.trim(), {
        key: v.key.trim(),
        value: v.value,
        secret: v.secret,
        scope: "全局变量",
      });
    }
  }
  for (const v of activeEnvironmentDetail.value?.variables ?? []) {
    if (v.active && v.key.trim()) {
      map.set(v.key.trim(), {
        key: v.key.trim(),
        value: v.value,
        secret: v.secret,
        scope: activeEnvironmentDetail.value?.name ?? "当前环境",
      });
    }
  }
  return Array.from(map.values());
});

/**
 * Load the detail of the currently selected environment. A response that
 * arrives after the selection moved on is discarded, so the variables shown
 * (and used for sending) always belong to the selected environment.
 */
let envDetailSeq = 0;

async function loadActiveEnvironmentDetail(): Promise<void> {
  const seq = ++envDetailSeq;
  const envId = activeEnvironmentId.value;
  if (!envId) {
    activeEnvironmentDetail.value = null;
    return;
  }
  // Never show the previous environment's variables under the new selection.
  if (activeEnvironmentDetail.value?.id !== envId) {
    activeEnvironmentDetail.value = null;
  }
  let detail: EnvironmentDetail | null;
  try {
    detail = await api.getEnvironment(envId);
  } catch {
    detail = null;
  }
  if (seq === envDetailSeq) {
    activeEnvironmentDetail.value = detail;
  }
}

watch(activeEnvironmentId, (envId) => {
  if (envId) {
    localStorage.setItem(ENV_STORAGE_KEY, envId);
  } else {
    localStorage.removeItem(ENV_STORAGE_KEY);
  }
  void loadActiveEnvironmentDetail();
});

export async function refreshCollections(): Promise<void> {
  collections.value = await api.listCollections();
  detachOrphanTabs(collections.value);
}

/**
 * Tabs whose saved request (or collection) no longer exists on the server are
 * turned back into unsaved drafts: their content is kept, the next save goes
 * through "save as" instead of failing with 404, and closing asks first.
 */
function detachOrphanTabs(tree: CollectionNode[]): void {
  const requestIds = new Set<string>();
  const collectionIds = new Set<string>();
  const walk = (nodes: CollectionNode[]): void => {
    for (const n of nodes) {
      collectionIds.add(n.id);
      for (const r of n.requests) requestIds.add(r.id);
      walk(n.folders);
    }
  };
  walk(tree);
  for (const tab of tabs.value) {
    const { id, collection_id: colId } = tab.draft;
    const requestGone = Boolean(id) && !requestIds.has(id!);
    const collectionGone = Boolean(colId) && !collectionIds.has(colId!);
    if (!requestGone && !collectionGone) continue;
    if (requestGone) tab.draft.id = null;
    if (collectionGone) tab.draft.collection_id = null;
    tab.savedSnapshotJson = null;
    tab.baselineSnapshotJson = JSON.stringify(
      normalizeDraftForApi(
        createDefaultRequestDraft(
          tab.draft.protocol === "mazbri" ? "mazbri" : "http",
        ),
      ),
    );
  }
}

export async function refreshEnvironments(): Promise<void> {
  const list = await api.listEnvironments();
  environments.value = list;
  const globalRow = list.find((e) => e.is_global);
  if (globalRow) {
    globalEnvironmentDetail.value = await api.getEnvironment(globalRow.id);
  }
  if (
    activeEnvironmentId.value &&
    !list.some((e) => e.id === activeEnvironmentId.value && !e.is_global)
  ) {
    // The watcher clears the detail.
    activeEnvironmentId.value = null;
    return;
  }
  await loadActiveEnvironmentDetail();
}

export async function refreshLoadProfiles(): Promise<void> {
  loadProfiles.value = await api.listLoadProfiles();
}

export async function refreshMeta(): Promise<void> {
  try {
    serverMeta.value = await api.meta({ silent: true });
  } catch {
    // Background poll: transient meta errors are ignored, never toasted.
  }
}

export async function bootstrapWorkspace(): Promise<void> {
  await Promise.allSettled([
    refreshCollections(),
    refreshEnvironments(),
    refreshLoadProfiles(),
    refreshMeta(),
  ]);
  workspaceLoaded.value = true;
}

// Tab operations
export function addNewRequestTab(protocol?: "http" | "mazbri"): void {
  const inherited =
    protocol ??
    (activeTab.value?.draft.protocol === "mazbri" ? "mazbri" : "http");
  const tab = createWorkspaceTab(createDefaultRequestDraft(inherited), false);
  tabs.value.push(tab);
  activeTabId.value = tab.tabId;
}

/** Path + query of an absolute URL; non-URLs are returned unchanged. */
function urlPathPart(endpoint: string): string {
  if (!/^https?:\/\//i.test(endpoint)) return endpoint;
  try {
    const u = new URL(endpoint);
    return `${u.pathname}${u.search}`;
  } catch {
    return endpoint;
  }
}

export function switchActiveTabProtocol(next?: "http" | "mazbri"): void {
  const tab = activeTab.value;
  const draft = tab.draft;
  const source: "http" | "mazbri" =
    draft.protocol === "mazbri" ? "mazbri" : "http";
  const target: "http" | "mazbri" =
    next ?? (source === "mazbri" ? "http" : "mazbri");
  if (source === target) return;

  const wasUntouched = !tab.savedSnapshotJson && !isTabDirty(tab);
  // Remember what the user typed for the protocol being left so that
  // switching back restores it verbatim.
  tab.protocolStash = {
    ...tab.protocolStash,
    [source]: {
      endpoint: draft.endpoint,
      method: draft.method,
      body: cloneJson(draft.body),
    },
  };
  draft.protocol = target;

  const stashed = tab.protocolStash[target];
  if (stashed) {
    draft.endpoint = stashed.endpoint;
    draft.method = stashed.method;
    draft.body = cloneJson(stashed.body);
  } else if (wasUntouched) {
    const defaults = createDefaultRequestDraft(target);
    draft.endpoint = defaults.endpoint;
    draft.method = defaults.method;
    draft.body = defaults.body;
  } else if (target === "mazbri") {
    // Mazbri endpoints are gateway paths; keep the user's path, drop the origin.
    draft.endpoint = urlPathPart(draft.endpoint);
  }

  if (target === "mazbri") {
    tab.requestSubTab = "auth";
  } else if (
    tab.requestSubTab === "auth" &&
    draft.auth.auth_type === "inherit"
  ) {
    tab.requestSubTab = "params";
  }
  // A response from the other protocol no longer describes this request.
  tab.response = null;

  if (wasUntouched) {
    tab.baselineSnapshotJson = JSON.stringify(normalizeDraftForApi(draft));
  }
}

export async function closeWorkspaceTab(tabId: string): Promise<void> {
  const target = tabs.value.find((t) => t.tabId === tabId);
  if (!target) return;

  if (isTabDirty(target)) {
    const confirmed = await askConfirm({
      title: "关闭未保存标签页",
      description: `「${target.draft.name || "未命名请求"}」存在未保存修改，确定放弃修改并关闭吗？`,
      confirmText: "放弃并关闭",
      variant: "destructive",
    });
    if (!confirmed) return;
  }

  if (tabs.value.length <= 1) {
    const fresh = createWorkspaceTab();
    tabs.value = [fresh];
    activeTabId.value = fresh.tabId;
    return;
  }
  const idx = tabs.value.findIndex((t) => t.tabId === tabId);
  if (idx < 0) return;
  const wasActive = activeTabId.value === tabId;
  tabs.value.splice(idx, 1);
  if (wasActive) {
    const next = tabs.value[Math.min(idx, tabs.value.length - 1)]!;
    activeTabId.value = next.tabId;
  }
}

export function duplicateWorkspaceTab(tabId: string): void {
  const src = tabs.value.find((t) => t.tabId === tabId);
  if (!src) return;
  const copyDraft = cloneJson(src.draft);
  copyDraft.id = null;
  copyDraft.name = `${copyDraft.name} (副本)`;
  const tab = createWorkspaceTab(copyDraft, false);
  tabs.value.push(tab);
  activeTabId.value = tab.tabId;
}

export function openSavedRequestInTab(req: SavedRequest): void {
  const existing = tabs.value.find((t) => t.draft.id === req.id);
  if (existing) {
    activeTabId.value = existing.tabId;
    return;
  }
  const draft: RequestDraft = cloneJson(req);
  const tab = createWorkspaceTab(draft, true);
  tabs.value.push(tab);
  activeTabId.value = tab.tabId;
}

export function openDraftInNewTab(draft: RequestDraft): void {
  const tab = createWorkspaceTab(cloneJson(draft), false);
  if (
    draft.body?.content_type &&
    (draft.method === "POST" ||
      draft.method === "PUT" ||
      draft.method === "PATCH")
  ) {
    tab.requestSubTab = "body";
  }
  tabs.value.push(tab);
  activeTabId.value = tab.tabId;
}

/**
 * Apply a server save result to the tab that issued the save.
 *
 * `sentJson` is the normalized draft that was sent. If the user kept editing
 * while the request was in flight, only the identity fields are adopted so the
 * newer edits survive (the tab then stays dirty against the saved snapshot).
 */
export function applySavedRequest(
  tab: WorkspaceTab,
  sentJson: string,
  saved: SavedRequest,
): void {
  const snap = JSON.stringify(normalizeDraftForApi(cloneJson(saved)));
  const editedMeanwhile =
    JSON.stringify(normalizeDraftForApi(tab.draft)) !== sentJson;
  if (editedMeanwhile) {
    tab.draft.id = saved.id;
    tab.draft.collection_id = saved.collection_id;
  } else {
    tab.draft = cloneJson(saved);
  }
  tab.savedSnapshotJson = snap;
  tab.baselineSnapshotJson = snap;
}

export async function saveActiveTab(): Promise<void> {
  const tab = activeTab.value;
  if (tab.draft.id && tab.draft.collection_id) {
    const normalized = normalizeDraftForApi(tab.draft);
    const sentJson = JSON.stringify(normalized);
    const saved = await api.updateRequest(tab.draft.id, normalized);
    applySavedRequest(tab, sentJson, saved);
    await refreshCollections();
    toast.success(`已保存「${saved.name}」`);
  } else {
    saveModalForceNew.value = false;
    saveModalOpen.value = true;
  }
}

export async function sendActiveTabProbe(): Promise<void> {
  const tab = activeTab.value;
  if (tab.sending) return;
  if (!tab.draft.endpoint.trim()) {
    toast.error(
      tab.draft.protocol === "mazbri" ? "请输入网关业务路径" : "请输入请求地址",
    );
    return;
  }
  responsePaneOpen.value = true;
  tab.sending = true;
  try {
    const normalized = normalizeDraftForApi(tab.draft);
    const res = await api.probe({
      request: normalized,
      collection_id: normalized.collection_id ?? null,
      environment_id: activeEnvironmentId.value,
    });
    tab.response = markRaw(res);
    probeHistoryVersion.value += 1;
    if (res.protocol === "mazbri" && !res.transport_ok && res.mazbri_wire) {
      tab.responseSubTab = "mazbri_wire";
    } else if (res.body_json !== undefined && res.body_json !== null) {
      tab.responseSubTab = "json";
    } else {
      tab.responseSubTab = "raw";
    }
  } finally {
    tab.sending = false;
  }
}

export function openEnvironmentEditor(envId: string | null): void {
  editingEnvId.value = envId;
  envModalOpen.value = true;
}

export function formatLocalTimestamp(date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

export function openProbeHistoryInNewTab(probe: ProbeRecord): void {
  const base = probe.request_draft
    ? cloneJson(probe.request_draft)
    : createDefaultRequestDraft(probe.protocol === "mazbri" ? "mazbri" : "http");
  // Detached independent draft: never affects saved requests in collections.
  base.id = null;
  base.collection_id = null;
  if (!probe.request_draft) {
    base.name = probe.request_name;
    base.method = probe.method;
    base.endpoint = probe.url;
  }
  const tab = createWorkspaceTab(base, false);
  if (
    base.body?.content_type &&
    (base.method === "POST" ||
      base.method === "PUT" ||
      base.method === "PATCH")
  ) {
    tab.requestSubTab = "body";
  }
  if (
    probe.environment_id &&
    selectableEnvironments.value.some((e) => e.id === probe.environment_id)
  ) {
    activeEnvironmentId.value = probe.environment_id;
  }
  tabs.value.push(tab);
  activeTabId.value = tab.tabId;
}

export function openRunReplayInWorkspace(replay: RunReplayPayload): "single" | "batch" {
  if (
    replay.environment_id &&
    selectableEnvironments.value.some((e) => e.id === replay.environment_id)
  ) {
    activeEnvironmentId.value = replay.environment_id;
  }
  const drafts = replay.requests.map((r) => {
    const d = cloneJson(r);
    d.id = null;
    d.collection_id = null;
    return d;
  });

  if (drafts.length <= 1) {
    const first = drafts[0] ?? createDefaultRequestDraft("http");
    const tab = createWorkspaceTab(first, false);
    tab.rerunConfig = {
      fromRunId: replay.run_id,
      fromTitle: replay.title,
      environmentId: replay.environment_id ?? null,
      loadConfig: cloneJson(replay.load_config),
      assertSpec: replay.assert ? cloneJson(replay.assert) : null,
    };
    if (
      first.body?.content_type &&
      (first.method === "POST" ||
        first.method === "PUT" ||
        first.method === "PATCH")
    ) {
      tab.requestSubTab = "body";
    }
    tabs.value.push(tab);
    activeTabId.value = tab.tabId;
    return "single";
  }

  runnerContext.value = {
    mode: "batch",
    batchDrafts: drafts,
    initialEnvId: replay.environment_id ?? null,
    initialLoadConfig: cloneJson(replay.load_config),
    initialAssertSpec: replay.assert ? cloneJson(replay.assert) : null,
    initialTitle: replay.title,
    fromRunId: replay.run_id,
  };
  runnerModalOpen.value = true;
  return "batch";
}

export function openSingleLoadRunner(draft?: RequestDraft): void {
  const tab = activeTab.value;
  const rerun = !draft || draft === tab.draft ? tab.rerunConfig : null;
  runnerContext.value = {
    mode: "single",
    draft: cloneJson(draft ?? tab.draft),
    initialEnvId: rerun?.environmentId ?? null,
    initialLoadConfig: rerun ? cloneJson(rerun.loadConfig) : null,
    initialAssertSpec: rerun?.assertSpec ? cloneJson(rerun.assertSpec) : null,
    initialTitle: rerun?.fromTitle ?? null,
    fromRunId: rerun?.fromRunId ?? null,
  };
  runnerModalOpen.value = true;
}

export function openCollectionLoadRunner(
  collectionId: string,
  collectionName: string,
): void {
  runnerContext.value = {
    mode: "collection",
    collectionId,
    collectionName,
  };
  runnerModalOpen.value = true;
}

// Collection tree lookup helpers
export function findCollectionBreadcrumb(
  nodes: CollectionNode[],
  targetId: string | null | undefined,
  prefix: string[] = [],
): string[] | null {
  if (!targetId) return null;
  for (const n of nodes) {
    const nextPath = [...prefix, n.name];
    if (n.id === targetId) return nextPath;
    const sub = findCollectionBreadcrumb(n.folders, targetId, nextPath);
    if (sub) return sub;
  }
  return null;
}

export function flattenCollectionOptions(
  nodes: CollectionNode[],
  depth = 0,
): { id: string; label: string; name: string }[] {
  const out: { id: string; label: string; name: string }[] = [];
  for (const n of nodes) {
    const indent = depth > 0 ? `${"  ".repeat(depth)}└ ` : "";
    out.push({ id: n.id, label: `${indent}${n.name}`, name: n.name });
    out.push(...flattenCollectionOptions(n.folders, depth + 1));
  }
  return out;
}

export function flattenCollectionRequests(
  node: CollectionNode,
  folderPrefix = "",
): { folderPath: string; request: SavedRequest }[] {
  const path = folderPrefix ? `${folderPrefix} / ${node.name}` : node.name;
  const out: { folderPath: string; request: SavedRequest }[] = node.requests.map(
    (request) => ({ folderPath: path, request }),
  );
  for (const child of node.folders) {
    out.push(...flattenCollectionRequests(child, path));
  }
  return out;
}

export function findCollectionById(
  nodes: CollectionNode[],
  id: string,
): CollectionNode | null {
  for (const n of nodes) {
    if (n.id === id) return n;
    const found = findCollectionById(n.folders, id);
    if (found) return found;
  }
  return null;
}
