<template>
  <UiDialog
    :open="runnerModalOpen"
    :title="modalTitle"
    description="启动时自动冻结当前请求、集合及环境快照，并跳转至实时吞吐监控大盘。"
    max-width="4xl"
    @close="runnerModalOpen = false"
  >
    <div class="space-y-5">
      <!-- Top Environment & Load Profile Selector -->
      <div
        class="grid grid-cols-1 gap-4 rounded-lg border border-border bg-surface/50 p-4 md:grid-cols-2"
      >
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            运行环境与密钥池 (Environment)
          </label>
          <UiSelect v-model="selectedEnvId">
            <option value="">不使用命名环境 (仅 Global 变量)</option>
            <option v-for="env in selectableEnvironments" :key="env.id" :value="env.id">
              {{ env.name }}
              {{ env.gateway_base_url ? `(${env.gateway_base_url})` : "" }}
            </option>
          </UiSelect>
        </div>

        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            载入已保存的负载预设 (Load Profile 可选)
          </label>
          <UiSelect
            :model-value="selectedProfileId"
            @update:model-value="applyProfilePreset"
          >
            <option value="">自定义参数 (不使用预设)</option>
            <option v-for="p in loadProfiles" :key="p.id" :value="p.id">
              {{ p.name }} ({{ p.model === "open_loop" ? `${p.qps} QPS` : "闭环" }} · {{ p.concurrency }} 并发)
            </option>
          </UiSelect>
        </div>
      </div>

      <!-- Single Request Summary OR Collection Weighted Request Selector -->
      <div
        v-if="runnerContext.mode === 'single' && runnerContext.draft"
        class="flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3"
      >
        <div class="flex min-w-0 items-center gap-2.5">
          <UiBadge variant="outline" class="uppercase font-mono">
            {{ runnerContext.draft.protocol }}
          </UiBadge>
          <span
            class="font-mono text-xs font-semibold shrink-0"
            :class="methodColorClass(runnerContext.draft.method)"
          >
            {{ runnerContext.draft.method }}
          </span>
          <span class="truncate font-mono text-sm text-foreground">
            {{ runnerContext.draft.endpoint }}
          </span>
        </div>
        <span class="shrink-0 text-xs text-muted-foreground">
          {{ runnerContext.draft.id ? "已保存请求" : "当前标签页草稿直压" }}
        </span>
      </div>

      <div
        v-else-if="runnerContext.mode === 'collection'"
        class="overflow-hidden rounded-lg border border-border bg-card"
      >
        <div
          class="flex items-center justify-between border-b border-border bg-muted/40 px-4 py-2.5"
        >
          <span class="text-xs font-semibold text-foreground">
            勾选参与集合混合压测的接口与流量权重 ({{ enabledCount }}/{{ collectionRows.length }})
          </span>
          <div class="flex items-center gap-1.5">
            <UiButton variant="ghost" size="xs" @click="toggleAll(true)">
              全选
            </UiButton>
            <UiButton variant="ghost" size="xs" @click="toggleAll(false)">
              全不选
            </UiButton>
          </div>
        </div>

        <div
          v-if="collectionRows.length === 0"
          class="p-8 text-center text-sm text-muted-foreground"
        >
          该集合内暂无已保存请求，请先在集合中创建或保存请求。
        </div>

        <div v-else class="max-h-56 divide-y divide-border overflow-y-auto">
          <div
            v-for="item in collectionRows"
            :key="item.request.id"
            class="flex items-center justify-between px-4 py-2.5 transition-colors hover:bg-muted/35"
          >
            <div class="flex min-w-0 flex-1 items-center gap-3">
              <input
                type="checkbox"
                :checked="item.enabled"
                :aria-label="`参与压测 ${item.request.name}`"
                class="h-4 w-4 cursor-pointer rounded border-input accent-foreground"
                @change="item.enabled = !item.enabled"
              />
              <UiBadge variant="muted" class="uppercase font-mono text-[10px]">
                {{ item.request.protocol }}
              </UiBadge>
              <span
                class="w-14 shrink-0 font-mono text-xs font-semibold"
                :class="methodColorClass(item.request.method)"
              >
                {{ item.request.method }}
              </span>
              <div class="min-w-0 truncate">
                <span class="mr-2 text-sm font-medium text-foreground">
                  {{ item.request.name }}
                </span>
                <span class="font-mono text-xs text-muted-foreground">
                  {{ item.request.endpoint }}
                </span>
              </div>
            </div>
            <div class="ml-3 flex shrink-0 items-center gap-2">
              <span class="text-xs text-muted-foreground whitespace-nowrap">权重</span>
              <input
                v-model.number="item.weight"
                type="number"
                min="1"
                :disabled="!item.enabled"
                class="h-8 w-20 rounded-md border border-input bg-background px-2.5 text-right font-mono text-xs text-foreground tabular-nums focus:outline-none focus:ring-1 focus:ring-ring disabled:opacity-50"
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Load Shaping Config -->
      <LoadConfigForm
        v-model="loadConfig"
        :assert-spec="assertSpec"
        @update:assert-spec="(v) => (assertSpec = v)"
      />
    </div>

    <template #footer>
      <div class="flex w-full items-center justify-between gap-3">
        <span class="hidden sm:inline text-xs text-muted-foreground">
          支持单进程或多进程集群并发，运行中可实时调整目标速率与并发上限
        </span>
        <div class="flex items-center gap-2 ml-auto">
          <UiButton variant="outline" @click="runnerModalOpen = false">
            取消
          </UiButton>
          <UiButton variant="default" :loading="launching" @click="launchRun">
            <Play v-if="!launching" class="h-3.5 w-3.5" />
            <span>立即启动压测</span>
          </UiButton>
        </div>
      </div>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Play } from "lucide-vue-next";
import { api } from "../../api/workspace";
import type {
  AssertSpec,
  CreateRunRequest,
  LoadConfig,
  SavedRequest,
} from "../../api/types";
import {
  cloneJson,
  createDefaultLoadConfig,
  methodColorClass,
  normalizeDraftForApi,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import {
  activeEnvironmentId,
  collections,
  findCollectionById,
  flattenCollectionRequests,
  loadProfiles,
  refreshMeta,
  runnerContext,
  runnerModalOpen,
  selectableEnvironments,
} from "../../stores/workspace";
import LoadConfigForm from "../common/LoadConfigForm.vue";
import UiBadge from "../ui/UiBadge.vue";
import UiButton from "../ui/UiButton.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiSelect from "../ui/UiSelect.vue";

interface SelectableRequestRow {
  folderPath: string;
  request: SavedRequest;
  enabled: boolean;
  weight: number;
}

const router = useRouter();
const selectedEnvId = ref("");
const selectedProfileId = ref("");
const loadConfig = ref<LoadConfig>(createDefaultLoadConfig());
const assertSpec = ref<AssertSpec | null>(null);
const collectionRows = ref<SelectableRequestRow[]>([]);
const launching = ref(false);

const modalTitle = computed(() => {
  if (runnerContext.value.mode === "collection") {
    return `集合加权混合压测：${runnerContext.value.collectionName ?? ""}`;
  }
  return `发起接口压测：${runnerContext.value.draft?.name || "当前请求"}`;
});

const enabledCount = computed(
  () => collectionRows.value.filter((r) => r.enabled).length,
);

watch(runnerModalOpen, (open) => {
  if (!open) return;
  selectedEnvId.value = activeEnvironmentId.value ?? "";
  selectedProfileId.value = "";
  loadConfig.value = createDefaultLoadConfig();
  assertSpec.value = null;

  if (
    runnerContext.value.mode === "collection" &&
    runnerContext.value.collectionId
  ) {
    const node = findCollectionById(
      collections.value,
      runnerContext.value.collectionId,
    );
    collectionRows.value = node
      ? flattenCollectionRequests(node).map((r) => ({
          ...r,
          enabled: true,
          weight: Math.max(1, r.request.weight || 1),
        }))
      : [];
  } else {
    collectionRows.value = [];
  }
});

function toggleAll(on: boolean): void {
  for (const row of collectionRows.value) {
    row.enabled = on;
  }
}

function applyProfilePreset(profileId: string): void {
  selectedProfileId.value = profileId;
  if (!profileId) return;
  const found = loadProfiles.value.find((p) => p.id === profileId);
  if (!found) return;
  loadConfig.value = {
    model: found.model,
    qps: found.qps ?? null,
    concurrency: found.concurrency,
    requests: found.requests ?? null,
    duration_secs: found.duration_secs ?? null,
    warmup_secs: found.warmup_secs,
    ramp_secs: found.ramp_secs,
    timeout_secs: found.timeout_secs,
    connect_timeout_secs: found.connect_timeout_secs,
    token_refresh_skew_secs: found.token_refresh_skew_secs,
    include_request_id: found.include_request_id,
    processes: found.processes,
    qps_mode: found.qps_mode,
    requests_mode: found.requests_mode,
    loop_enabled: found.loop_enabled,
    rounds: found.rounds ?? null,
    loop_interval_secs: found.loop_interval_secs,
    expected_reasons: cloneJson(found.expected_reasons ?? []),
    detail_retain_days: found.detail_retain_days ?? null,
    detail_retain_rows: found.detail_retain_rows ?? null,
  };
  assertSpec.value = found.assert ? cloneJson(found.assert) : null;
}

async function launchRun(): Promise<void> {
  const cfg = loadConfig.value;
  const body: CreateRunRequest = {
    environment_id: selectedEnvId.value || null,
    load_profile_id: selectedProfileId.value || null,
    mode: cfg.loop_enabled ? "loop" : "once",
    model: cfg.model,
    qps: cfg.model === "open_loop" ? (cfg.qps ?? 20) : null,
    concurrency: cfg.concurrency,
    requests: cfg.requests ?? null,
    duration_secs: cfg.duration_secs ?? null,
    warmup_secs: cfg.warmup_secs,
    ramp_secs: cfg.ramp_secs,
    timeout_secs: cfg.timeout_secs,
    connect_timeout_secs: cfg.connect_timeout_secs,
    token_refresh_skew_secs: cfg.token_refresh_skew_secs,
    include_request_id: cfg.include_request_id,
    processes: cfg.processes,
    qps_mode: cfg.qps_mode,
    requests_mode: cfg.requests_mode,
    loop_enabled: cfg.loop_enabled,
    rounds: cfg.loop_enabled ? (cfg.rounds ?? null) : null,
    clear_rounds: cfg.loop_enabled && !cfg.rounds ? true : null,
    loop_interval_secs: cfg.loop_interval_secs,
    expected_reasons: cfg.expected_reasons,
    assert: assertSpec.value,
  };

  if (runnerContext.value.mode === "collection") {
    const selected = collectionRows.value
      .filter((r) => r.enabled)
      .map((r) => ({
        request_id: r.request.id,
        weight: Math.max(1, Number(r.weight) || 1),
      }));
    if (selected.length === 0) {
      toast.error("请至少勾选一个要参与压测的请求");
      return;
    }
    body.collection_id = runnerContext.value.collectionId ?? null;
    body.selected_requests = selected;
  } else if (runnerContext.value.draft) {
    body.adhoc_request = normalizeDraftForApi(runnerContext.value.draft);
  }

  launching.value = true;
  try {
    const created = await api.createRun(body);
    runnerModalOpen.value = false;
    void refreshMeta();
    toast.success(`压测任务已启动 (${created.id.slice(0, 8)})`);
    await router.push({ name: "run-detail", params: { id: created.id } });
  } finally {
    launching.value = false;
  }
}
</script>
