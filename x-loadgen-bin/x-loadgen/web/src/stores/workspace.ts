import { computed, ref, watch } from "vue";
import { api } from "../api/workspace";
import type {
  CollectionNode,
  EnvironmentDetail,
  EnvironmentSummary,
  LoadProfile,
  MetaResponse,
  ProbeResponse,
  RequestDraft,
  SavedRequest,
} from "../api/types";
import {
  cloneJson,
  createDefaultRequestDraft,
  normalizeDraftForApi,
} from "../helpers/format";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";

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
}

export type RightSidebarSection = "collections" | "environments" | "profiles";

export interface LoadRunnerLaunchContext {
  mode: "single" | "collection";
  draft?: RequestDraft;
  collectionId?: string;
  collectionName?: string;
}

const TABS_STORAGE_KEY = "x-loadgen.workspace.tabs.v3";
const ACTIVE_TAB_STORAGE_KEY = "x-loadgen.workspace.activeTab.v2";
const ENV_STORAGE_KEY = "x-loadgen.workspace.envId.v2";
const LAYOUT_STORAGE_KEY = "x-loadgen.workspace.layout.v3";

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
    return parsed.map((item) => {
      const draft = item.draft ?? createDefaultRequestDraft("http");
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
        savedSnapshotJson: item.savedSnapshotJson ?? null,
        baselineSnapshotJson: item.baselineSnapshotJson ?? defaultBaseline,
        requestSubTab: item.requestSubTab ?? "params",
        responseSubTab: item.responseSubTab ?? "json",
        sending: false,
        response: item.response ?? null,
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
    return {
      leftExpanded: p.leftExpanded !== false,
      rightOpen: p.rightOpen !== false,
      rightSection: p.rightSection ?? "collections",
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

export function isTabDirty(tab: WorkspaceTab): boolean {
  const current = JSON.stringify(normalizeDraftForApi(tab.draft));
  if (tab.savedSnapshotJson) {
    return current !== tab.savedSnapshotJson;
  }
  return current !== tab.baselineSnapshotJson;
}

// Layout state (responsePaneOpen defaults to false so bottom response bar is hidden initially)
export const leftSidebarExpanded = ref<boolean>(initialLayout.leftExpanded);
export const rightSidebarOpen = ref<boolean>(initialLayout.rightOpen);
export const rightSidebarSection = ref<RightSidebarSection>(
  initialLayout.rightSection,
);
export const horizontalSplit = ref<boolean>(initialLayout.horizontalSplit);
export const responsePaneOpen = ref<boolean>(false);

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

watch(
  tabs,
  (list) => {
    try {
      const compact = list.map((t) => ({
        tabId: t.tabId,
        draft: t.draft,
        savedSnapshotJson: t.savedSnapshotJson,
        baselineSnapshotJson: t.baselineSnapshotJson,
        requestSubTab: t.requestSubTab,
        responseSubTab: t.responseSubTab,
      }));
      localStorage.setItem(TABS_STORAGE_KEY, JSON.stringify(compact));
    } catch {
      // Ignore storage quota errors
    }
  },
  { deep: true },
);

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

watch(activeEnvironmentId, async (envId) => {
  if (envId) {
    localStorage.setItem(ENV_STORAGE_KEY, envId);
    try {
      activeEnvironmentDetail.value = await api.getEnvironment(envId);
    } catch {
      activeEnvironmentDetail.value = null;
    }
  } else {
    localStorage.removeItem(ENV_STORAGE_KEY);
    activeEnvironmentDetail.value = null;
  }
});

export async function refreshCollections(): Promise<void> {
  collections.value = await api.listCollections();
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
    activeEnvironmentId.value = null;
  }
  if (activeEnvironmentId.value) {
    activeEnvironmentDetail.value = await api.getEnvironment(
      activeEnvironmentId.value,
    );
  }
}

export async function refreshLoadProfiles(): Promise<void> {
  loadProfiles.value = await api.listLoadProfiles();
}

export async function refreshMeta(): Promise<void> {
  try {
    serverMeta.value = await api.meta();
  } catch {
    // Ignore transient meta errors
  }
}

export async function bootstrapWorkspace(): Promise<void> {
  await Promise.all([
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

export function switchActiveTabProtocol(next?: "http" | "mazbri"): void {
  const tab = activeTab.value;
  const draft = tab.draft;
  const target: "http" | "mazbri" =
    next ?? (draft.protocol === "mazbri" ? "http" : "mazbri");
  if (draft.protocol === target) return;

  const wasUntouched = !tab.savedSnapshotJson && !isTabDirty(tab);
  draft.protocol = target;

  if (target === "mazbri") {
    tab.requestSubTab = "auth";
    if (!draft.endpoint || draft.endpoint.startsWith("http")) {
      draft.endpoint = "/v1/demo";
    }
    if (draft.method === "GET" && !draft.body.content_type) {
      draft.method = "POST";
      draft.body.content_type = "application/json";
      if (!draft.body.body) {
        draft.body.body = '{\n  "ping": "mazbri"\n}';
      }
    }
  } else {
    if (draft.endpoint === "/v1/demo") {
      draft.endpoint = "";
    }
    if (tab.requestSubTab === "auth" && draft.auth.auth_type === "inherit") {
      tab.requestSubTab = "params";
    }
  }

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

export async function saveActiveTab(): Promise<void> {
  const tab = activeTab.value;
  if (tab.draft.id && tab.draft.collection_id) {
    const normalized = normalizeDraftForApi(tab.draft);
    const saved = await api.updateRequest(tab.draft.id, normalized);
    tab.draft = cloneJson(saved);
    const snap = JSON.stringify(normalizeDraftForApi(tab.draft));
    tab.savedSnapshotJson = snap;
    tab.baselineSnapshotJson = snap;
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
    tab.response = res;
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

export function openSingleLoadRunner(draft?: RequestDraft): void {
  runnerContext.value = {
    mode: "single",
    draft: cloneJson(draft ?? activeTab.value.draft),
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
