<template>
  <aside class="flex min-h-0 flex-1 overflow-hidden bg-background select-none">
    <!-- Secondary Icon Rail -->
    <div
      class="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-border bg-surface/60 py-2"
    >
      <button
        v-for="sec in sections"
        :key="sec.id"
        type="button"
        :title="sec.label"
        :aria-label="sec.label"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-md transition-colors"
        :class="
          rightSidebarSection === sec.id
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'
        "
        @click="rightSidebarSection = sec.id"
      >
        <component :is="sec.icon" class="h-4 w-4" />
      </button>
    </div>

    <!-- Active Sidebar Content -->
    <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <!-- 1. COLLECTIONS SECTION -->
      <template v-if="rightSidebarSection === 'collections'">
        <div class="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
          <span class="text-xs font-semibold tracking-tight text-foreground">
            接口集合
          </span>
          <div class="flex items-center gap-0.5">
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="新建集合"
              aria-label="新建集合"
              @click="openCreateCollection(null)"
            >
              <FolderPlus class="h-3.5 w-3.5" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="导入集合 JSON"
              aria-label="导入集合 JSON"
              @click="triggerImport('collections')"
            >
              <Upload class="h-3.5 w-3.5" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="导出集合 JSON"
              aria-label="导出集合 JSON"
              @click="exportData('collections')"
            >
              <Download class="h-3.5 w-3.5" />
            </UiButton>
          </div>
        </div>

        <!-- Inline Quick Create Collection Input -->
        <div
          v-if="creatingCollection"
          class="space-y-2 border-b border-border bg-surface/60 p-3"
        >
          <div class="text-xs font-medium text-muted-foreground">
            {{
              parentForNewCollection
                ? `新建子目录于「${parentForNewCollection.name}」`
                : "新建集合"
            }}
          </div>
          <UiInput
            v-model="newCollectionName"
            size="sm"
            placeholder="输入集合名称..."
            @keydown.enter.prevent="submitCreateCollection"
          />
          <div class="flex items-center justify-end gap-1.5">
            <UiButton variant="ghost" size="xs" @click="creatingCollection = false">
              取消
            </UiButton>
            <UiButton variant="default" size="xs" @click="submitCreateCollection">
              创建
            </UiButton>
          </div>
        </div>

        <div class="border-b border-border p-2.5">
          <UiSearchInput
            v-model="searchText"
            placeholder="搜索集合或请求..."
          />
        </div>

        <div class="flex-1 overflow-y-auto p-2">
          <div
            v-if="filteredCollections.length === 0"
            class="flex flex-col items-center justify-center gap-3 px-4 py-12 text-center text-muted-foreground"
          >
            <Folder class="h-8 w-8 opacity-35" />
            <p class="text-xs leading-relaxed">
              {{ searchText.trim() ? "无匹配的集合或请求" : "暂无集合，可将请求归档并发起加权压测" }}
            </p>
            <UiButton
              v-if="!searchText.trim()"
              variant="outline"
              size="sm"
              @click="openCreateCollection(null)"
            >
              <FolderPlus class="h-3.5 w-3.5" />
              <span>新建集合</span>
            </UiButton>
          </div>

          <CollectionTreeNode
            v-for="col in filteredCollections"
            :key="col.id"
            :node="col"
            @create-request="createNewRequestInside"
            @create-subfolder="openCreateCollection"
            @rename-collection="handleRenameCollection"
            @delete-collection="handleDeleteCollection"
            @delete-request="handleDeleteRequest"
          />
        </div>
      </template>

      <!-- 2. ENVIRONMENTS & TENANT POOLS SECTION -->
      <template v-else-if="rightSidebarSection === 'environments'">
        <div class="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
          <span class="text-xs font-semibold tracking-tight text-foreground">
            环境与密钥池
          </span>
          <div class="flex items-center gap-0.5">
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="新建环境"
              aria-label="新建环境"
              @click="openEnvironmentEditor(null)"
            >
              <Plus class="h-3.5 w-3.5" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="导入环境 JSON"
              aria-label="导入环境 JSON"
              @click="triggerImport('environments')"
            >
              <Upload class="h-3.5 w-3.5" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="导出环境 JSON"
              aria-label="导出环境 JSON"
              @click="exportData('environments')"
            >
              <Download class="h-3.5 w-3.5" />
            </UiButton>
          </div>
        </div>

        <div
          class="flex items-center justify-between border-b border-border bg-surface/40 px-3 py-2 text-xs"
        >
          <span class="text-muted-foreground">导出时包含明文密钥</span>
          <UiSwitch v-model="includeSecretsOnExport" />
        </div>

        <div class="flex-1 space-y-2 overflow-y-auto p-2.5">
          <!-- Global Row -->
          <div
            v-if="globalEnvRow"
            class="flex cursor-pointer items-center justify-between rounded-lg border border-border bg-card p-3 transition-colors hover:bg-accent/50"
            @click="openEnvironmentEditor(globalEnvRow.id)"
          >
            <div>
              <div class="text-xs font-semibold text-foreground">
                全局变量
              </div>
              <div class="mt-0.5 font-mono text-[11px] text-muted-foreground">
                {{ globalEnvRow.variable_count }} 个变量
              </div>
            </div>
            <Edit3 class="h-3.5 w-3.5 text-muted-foreground" />
          </div>

          <!-- Named Environments -->
          <div
            v-for="env in selectableEnvironments"
            :key="env.id"
            class="flex cursor-pointer items-center justify-between rounded-lg border p-3 transition-colors"
            :class="
              activeEnvironmentId === env.id
                ? 'border-foreground bg-accent/40'
                : 'border-border bg-card hover:bg-accent/40'
            "
            @click="activeEnvironmentId = env.id"
          >
            <div class="min-w-0 pr-2">
              <div class="flex items-center gap-2">
                <span
                  class="h-2 w-2 shrink-0 rounded-full"
                  :class="
                    activeEnvironmentId === env.id
                      ? 'bg-foreground'
                      : 'bg-muted-foreground/40'
                  "
                />
                <span class="truncate text-xs font-semibold text-foreground">
                  {{ env.name }}
                </span>
              </div>
              <div class="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                {{ env.variable_count }} 变量 · {{ env.tenant_count }} 租户
              </div>
            </div>

            <UiButton
              variant="ghost"
              size="icon-xs"
              title="编辑环境"
              @click.stop="openEnvironmentEditor(env.id)"
            >
              <Edit3 class="h-3.5 w-3.5" />
            </UiButton>
          </div>
        </div>
      </template>

      <!-- 3. LOAD PROFILES QUICK LIST -->
      <template v-else>
        <div class="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
          <span class="text-xs font-semibold tracking-tight text-foreground">
            负载预设
          </span>
          <UiButton
            variant="ghost"
            size="icon-xs"
            title="负载预设"
            @click="router.push({ name: 'profiles' })"
          >
            <ExternalLink class="h-3.5 w-3.5" />
          </UiButton>
        </div>

        <div class="border-b border-border p-2.5">
          <UiSearchInput
            v-model="profileSearchText"
            placeholder="搜索预设名称..."
          />
        </div>

        <div class="flex-1 space-y-2 overflow-y-auto p-2.5">
          <div
            v-if="filteredProfiles.length === 0"
            class="space-y-3 py-10 text-center text-xs text-muted-foreground"
          >
            <p>{{ profileSearchText.trim() ? "无匹配的负载预设" : "暂无负载预设" }}</p>
            <UiButton
              v-if="!profileSearchText.trim()"
              variant="outline"
              size="sm"
              @click="router.push({ name: 'profiles' })"
            >
              新建预设
            </UiButton>
          </div>
          <div
            v-for="p in filteredProfiles"
            :key="p.id"
            class="cursor-pointer rounded-lg border border-border bg-card p-3 transition-colors hover:bg-accent/50"
            @click="router.push({ name: 'profiles' })"
          >
            <div class="text-xs font-semibold text-foreground">{{ p.name }}</div>
            <div class="mt-1 font-mono text-[11px] text-muted-foreground tabular-nums">
              {{ p.model === "open_loop" ? `${p.qps} QPS` : "闭环" }} ·
              {{ p.concurrency }} 并发 · {{ p.processes }} 进程
            </div>
          </div>
        </div>
      </template>
    </div>

    <input
      ref="fileInputEl"
      type="file"
      accept="application/json,.json"
      class="hidden"
      @change="onFileSelected"
    />
  </aside>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import {
  Download,
  Edit3,
  ExternalLink,
  Folder,
  FolderPlus,
  Gauge,
  Layers,
  Plus,
  Upload,
} from "lucide-vue-next";
import { api } from "../../api/workspace";
import type { CollectionNode, SavedRequest } from "../../api/types";
import { createDefaultRequestDraft } from "../../helpers/format";
import { askConfirm } from "../../modules/confirm";
import { toast } from "../../modules/toast";
import {
  activeEnvironmentId,
  collections,
  environments,
  loadProfiles,
  openEnvironmentEditor,
  openSavedRequestInTab,
  refreshCollections,
  refreshEnvironments,
  rightSidebarSection,
  selectableEnvironments,
  type RightSidebarSection,
} from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import UiSearchInput from "../ui/UiSearchInput.vue";
import UiSwitch from "../ui/UiSwitch.vue";
import CollectionTreeNode from "./CollectionTreeNode.vue";

const router = useRouter();
const searchText = ref("");
const profileSearchText = ref("");
const creatingCollection = ref(false);
const newCollectionName = ref("");
const parentForNewCollection = ref<CollectionNode | null>(null);
const includeSecretsOnExport = ref(false);
const fileInputEl = ref<HTMLInputElement | null>(null);
const pendingImportKind = ref<"collections" | "environments">("collections");

const sections: { id: RightSidebarSection; label: string; icon: unknown }[] = [
  { id: "collections", label: "接口集合", icon: Folder },
  { id: "environments", label: "环境与密钥池", icon: Layers },
  { id: "profiles", label: "负载预设", icon: Gauge },
];

const globalEnvRow = computed(() =>
  environments.value.find((e) => e.is_global),
);

function filterTree(nodes: CollectionNode[], q: string): CollectionNode[] {
  if (!q) return nodes;
  const lower = q.toLowerCase();
  const out: CollectionNode[] = [];
  for (const n of nodes) {
    const nameMatch = n.name.toLowerCase().includes(lower);
    const subFolders = filterTree(n.folders, q);
    const subReqs = n.requests.filter(
      (r) =>
        r.name.toLowerCase().includes(lower) ||
        r.endpoint.toLowerCase().includes(lower),
    );
    if (nameMatch || subFolders.length > 0 || subReqs.length > 0) {
      out.push({
        ...n,
        folders: nameMatch ? n.folders : subFolders,
        requests: nameMatch ? n.requests : subReqs,
      });
    }
  }
  return out;
}

const filteredCollections = computed(() =>
  filterTree(collections.value, searchText.value.trim()),
);

const filteredProfiles = computed(() => {
  const q = profileSearchText.value.trim().toLowerCase();
  if (!q) return loadProfiles.value;
  return loadProfiles.value.filter(
    (p) =>
      p.name.toLowerCase().includes(q) ||
      (p.note ?? "").toLowerCase().includes(q),
  );
});

function openCreateCollection(parent: CollectionNode | null): void {
  parentForNewCollection.value = parent;
  newCollectionName.value = "";
  creatingCollection.value = true;
}

async function submitCreateCollection(): Promise<void> {
  const name = newCollectionName.value.trim();
  if (!name) return;
  await api.createCollection({
    name,
    parent_id: parentForNewCollection.value?.id ?? null,
    default_protocol: parentForNewCollection.value?.default_protocol ?? "http",
  });
  creatingCollection.value = false;
  newCollectionName.value = "";
  await refreshCollections();
  toast.success(`已创建集合「${name}」`);
}

async function handleRenameCollection(
  node: CollectionNode,
  newName: string,
): Promise<void> {
  await api.updateCollection(node.id, {
    parent_id: node.parent_id ?? null,
    name: newName,
    sort_order: node.sort_order,
    default_protocol: node.default_protocol,
    auth: node.auth,
    headers: node.headers,
    variables: node.variables,
    description: node.description ?? null,
  });
  await refreshCollections();
  toast.success(`集合已重命名为「${newName}」`);
}

async function createNewRequestInside(node: CollectionNode): Promise<void> {
  const proto = node.default_protocol === "mazbri" ? "mazbri" : "http";
  const draft = createDefaultRequestDraft(proto);
  draft.collection_id = node.id;
  draft.endpoint = proto === "mazbri" ? "/v1/demo" : "/health";
  const created = await api.createRequest(draft);
  await refreshCollections();
  openSavedRequestInTab(created);
  toast.success(`已在「${node.name}」下新建请求`);
}

async function handleDeleteCollection(node: CollectionNode): Promise<void> {
  const confirmed = await askConfirm({
    title: "删除集合",
    description: `确定删除集合「${node.name}」及其下属全部请求吗？`,
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  await api.deleteCollection(node.id);
  await refreshCollections();
  toast.success("集合已删除");
}

async function handleDeleteRequest(req: SavedRequest): Promise<void> {
  const confirmed = await askConfirm({
    title: "删除请求",
    description: `确定删除请求「${req.name}」吗？`,
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  await api.deleteRequest(req.id);
  await refreshCollections();
  toast.success("请求已删除");
}

function triggerImport(kind: "collections" | "environments"): void {
  pendingImportKind.value = kind;
  if (fileInputEl.value) {
    fileInputEl.value.value = "";
    fileInputEl.value.click();
  }
}

async function onFileSelected(ev: Event): Promise<void> {
  const file = (ev.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    const parsed: unknown = JSON.parse(text);
    if (pendingImportKind.value === "collections") {
      await api.importCollections(parsed);
      await refreshCollections();
      toast.success("集合导入成功");
    } else {
      await api.importEnvironments(parsed);
      await refreshEnvironments();
      toast.success("环境导入成功");
    }
  } catch (err) {
    toast.error(
      `导入失败：${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

async function exportData(kind: "collections" | "environments"): Promise<void> {
  const data =
    kind === "collections"
      ? await api.exportCollections({
          include_secrets: includeSecretsOnExport.value,
        })
      : await api.exportEnvironments({
          include_secrets: includeSecretsOnExport.value,
        });
  const blob = new Blob([JSON.stringify(data, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `x-loadgen-${kind}-${Date.now()}.json`;
  a.click();
  URL.revokeObjectURL(url);
  toast.success(`已导出${kind === "collections" ? "集合" : "环境"}配置`);
}
</script>
