<template>
  <aside class="flex h-full border-l border-dividerLight bg-primary select-none overflow-hidden">
    <!-- Hoppscotch Secondary Rail -->
    <div class="flex flex-col items-center py-2 w-11 border-r border-dividerLight bg-primary space-y-1 flex-shrink-0">
      <button
        v-for="sec in sections"
        :key="sec.id"
        v-tippy="sec.label"
        type="button"
        class="p-2 rounded transition"
        :class="
          rightSidebarSection === sec.id
            ? 'text-accent bg-primaryLight'
            : 'text-secondaryLight hover:text-secondaryDark'
        "
        @click="rightSidebarSection = sec.id"
      >
        <component :is="sec.icon" class="w-4 h-4" />
      </button>
    </div>

    <!-- Active Sidebar Content -->
    <div class="flex flex-col flex-1 min-w-0 overflow-hidden">
      <!-- 1. COLLECTIONS SECTION -->
      <template v-if="rightSidebarSection === 'collections'">
        <div class="flex items-center justify-between px-3 py-2 border-b border-dividerLight">
          <span class="font-bold text-tiny text-secondaryDark uppercase tracking-wider">
            接口集合 (Collections)
          </span>
          <div class="flex items-center space-x-0.5">
            <HoppButtonSecondary
              v-tippy="'新建根集合'"
              :icon="FolderPlus"
              @click="openCreateCollection(null)"
            />
            <HoppButtonSecondary
              v-tippy="'导入 Hoppscotch / x-loadgen JSON 集合'"
              :icon="Upload"
              @click="triggerImport('collections')"
            />
            <HoppButtonSecondary
              v-tippy="'导出全部集合 JSON'"
              :icon="Download"
              @click="exportData('collections')"
            />
          </div>
        </div>

        <!-- Inline Quick Create Collection Input -->
        <div
          v-if="creatingCollection"
          class="p-2.5 border-b border-dividerLight bg-primaryLight/50 space-y-2"
        >
          <div class="text-tiny text-secondaryLight font-semibold">
            {{ parentForNewCollection ? `新建子目录于「${parentForNewCollection.name}」` : "新建根集合" }}
          </div>
          <input
            v-model="newCollectionName"
            type="text"
            placeholder="输入集合名称..."
            class="input !py-1 text-tiny"
            @keydown.enter.prevent="submitCreateCollection"
          />
          <div class="flex items-center justify-end space-x-1">
            <HoppButtonSecondary label="取消" @click="creatingCollection = false" />
            <HoppButtonPrimary label="创建" @click="submitCreateCollection" />
          </div>
        </div>

        <div class="px-2.5 py-2 border-b border-dividerLight">
          <input
            v-model="searchText"
            type="text"
            placeholder="搜索集合或请求..."
            class="input !py-1 !px-2.5 text-tiny"
          />
        </div>

        <div class="flex-1 overflow-y-auto p-1.5">
          <div
            v-if="filteredCollections.length === 0"
            class="flex flex-col items-center justify-center py-10 px-4 text-center text-secondaryLight text-tiny space-y-3"
          >
            <Folder class="w-8 h-8 opacity-30" />
            <p>暂无集合，可将调试好的 HTTP 或 Mazbri 请求归档并发起集合混合压测</p>
            <HoppButtonSecondary
              :icon="FolderPlus"
              label="新建首个集合"
              outline
              @click="openCreateCollection(null)"
            />
          </div>

          <CollectionTreeNode
            v-for="col in filteredCollections"
            :key="col.id"
            :node="col"
            @create-request="createNewRequestInside"
            @create-subfolder="openCreateCollection"
            @delete-collection="handleDeleteCollection"
            @delete-request="handleDeleteRequest"
          />
        </div>
      </template>

      <!-- 2. ENVIRONMENTS & TENANT POOLS SECTION -->
      <template v-else-if="rightSidebarSection === 'environments'">
        <div class="flex items-center justify-between px-3 py-2 border-b border-dividerLight">
          <span class="font-bold text-tiny text-secondaryDark uppercase tracking-wider">
            环境与密钥池 (Environments)
          </span>
          <div class="flex items-center space-x-0.5">
            <HoppButtonSecondary
              v-tippy="'新建环境'"
              :icon="Plus"
              @click="openEnvironmentEditor(null)"
            />
            <HoppButtonSecondary
              v-tippy="'导入环境 JSON'"
              :icon="Upload"
              @click="triggerImport('environments')"
            />
            <HoppButtonSecondary
              v-tippy="'导出环境 JSON'"
              :icon="Download"
              @click="exportData('environments')"
            />
          </div>
        </div>

        <div class="px-3 py-1.5 border-b border-dividerLight flex items-center justify-between text-tiny">
          <span class="text-secondaryLight">导出时包含明文密钥</span>
          <HoppSmartCheckbox
            :on="includeSecretsOnExport"
            name="export-secrets"
            @change="includeSecretsOnExport = !includeSecretsOnExport"
          />
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1.5">
          <!-- Global Row -->
          <div
            v-if="globalEnvRow"
            class="p-2.5 rounded border border-dividerLight bg-primaryLight/40 flex items-center justify-between cursor-pointer hover:border-accent transition"
            @click="openEnvironmentEditor(globalEnvRow.id)"
          >
            <div>
              <div class="font-semibold text-secondaryDark text-tiny">Global (全局变量)</div>
              <div class="text-[11px] text-secondaryLight font-mono">
                {{ globalEnvRow.variable_count }} 个全局变量
              </div>
            </div>
            <Edit3 class="w-3.5 h-3.5 text-secondaryLight" />
          </div>

          <!-- Named Environments -->
          <div
            v-for="env in selectableEnvironments"
            :key="env.id"
            class="p-2.5 rounded border transition flex items-center justify-between cursor-pointer"
            :class="
              activeEnvironmentId === env.id
                ? 'border-accent bg-accent/10'
                : 'border-dividerLight hover:bg-primaryLight/50'
            "
            @click="activeEnvironmentId = env.id"
          >
            <div class="min-w-0 pr-2">
              <div class="flex items-center space-x-1.5">
                <span
                  class="w-2 h-2 rounded-full flex-shrink-0"
                  :class="activeEnvironmentId === env.id ? 'bg-accent' : 'bg-dividerDark'"
                />
                <span class="font-semibold text-secondaryDark text-tiny truncate">
                  {{ env.name }}
                </span>
              </div>
              <div class="text-[11px] text-secondaryLight font-mono truncate mt-0.5">
                {{ env.variable_count }} 变量 · {{ env.tenant_count }} Mazbri 租户
              </div>
            </div>

            <HoppButtonSecondary
              v-tippy="'编辑变量与 Mazbri 租户密钥池'"
              :icon="Edit3"
              @click.stop="openEnvironmentEditor(env.id)"
            />
          </div>
        </div>
      </template>

      <!-- 3. LOAD PROFILES QUICK LIST -->
      <template v-else>
        <div class="flex items-center justify-between px-3 py-2 border-b border-dividerLight">
          <span class="font-bold text-tiny text-secondaryDark uppercase tracking-wider">
            压测配置模板 (Profiles)
          </span>
          <HoppButtonSecondary
            v-tippy="'前往模板管理页'"
            :icon="ExternalLink"
            @click="router.push({ name: 'profiles' })"
          />
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1.5">
          <div v-if="loadProfiles.length === 0" class="py-8 text-center text-secondaryLight text-tiny space-y-3">
            <p>暂无已保存的压测模板</p>
            <HoppButtonSecondary
              label="前往创建模板"
              outline
              @click="router.push({ name: 'profiles' })"
            />
          </div>
          <div
            v-for="p in loadProfiles"
            :key="p.id"
            class="p-2.5 rounded border border-dividerLight hover:border-accent transition cursor-pointer"
            @click="router.push({ name: 'profiles' })"
          >
            <div class="font-semibold text-secondaryDark text-tiny">{{ p.name }}</div>
            <div class="text-[11px] text-secondaryLight font-mono mt-0.5">
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
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartCheckbox,
} from "@hoppscotch/ui";
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
import CollectionTreeNode from "./CollectionTreeNode.vue";

const router = useRouter();
const searchText = ref("");
const creatingCollection = ref(false);
const newCollectionName = ref("");
const parentForNewCollection = ref<CollectionNode | null>(null);
const includeSecretsOnExport = ref(false);
const fileInputEl = ref<HTMLInputElement | null>(null);
const pendingImportKind = ref<"collections" | "environments">("collections");

const sections: { id: RightSidebarSection; label: string; icon: unknown }[] = [
  { id: "collections", label: "集合与目录 (Collections)", icon: Folder },
  { id: "environments", label: "环境与密钥池 (Environments)", icon: Layers },
  { id: "profiles", label: "压测模板 (Load Profiles)", icon: Gauge },
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

async function createNewRequestInside(node: CollectionNode): Promise<void> {
  const proto = node.default_protocol === "mazbri" ? "mazbri" : "http";
  const draft = createDefaultRequestDraft(proto);
  draft.collection_id = node.id;
  const created = await api.createRequest(draft);
  await refreshCollections();
  openSavedRequestInTab(created);
  toast.success(`已在「${node.name}」下新建请求`);
}

async function handleDeleteCollection(node: CollectionNode): Promise<void> {
  if (!confirm(`确定删除集合「${node.name}」及其下属全部请求吗？`)) return;
  await api.deleteCollection(node.id);
  await refreshCollections();
  toast.success("集合已删除");
}

async function handleDeleteRequest(req: SavedRequest): Promise<void> {
  if (!confirm(`确定删除请求「${req.name}」吗？`)) return;
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
      toast.success("集合 JSON 导入成功");
    } else {
      await api.importEnvironments(parsed);
      await refreshEnvironments();
      toast.success("环境 JSON 导入成功");
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
  toast.success(`已导出 ${kind} JSON`);
}
</script>
