<template>
  <HoppSmartModal
    v-if="runnerModalOpen"
    dialog
    :title="modalTitle"
    styles="sm:max-w-4xl"
    @close="runnerModalOpen = false"
  >
    <template #body>
      <div class="space-y-4">
        <!-- Top Environment & Load Profile Selector -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3 p-3 rounded border border-dividerLight bg-primaryLight/40">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              运行环境与密钥池 (Environment)
            </label>
            <select v-model="selectedEnvId" class="select">
              <option value="">不使用命名环境 (仅 Global 变量)</option>
              <option v-for="env in selectableEnvironments" :key="env.id" :value="env.id">
                {{ env.name }}
                {{ env.gateway_base_url ? `(${env.gateway_base_url})` : "" }}
              </option>
            </select>
          </div>

          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              载入已保存的压测模板预设 (Load Profile 可选)
            </label>
            <select
              :value="selectedProfileId"
              class="select"
              @change="applyProfilePreset(($event.target as HTMLSelectElement).value)"
            >
              <option value="">自定义参数 (不使用预设模板)</option>
              <option v-for="p in loadProfiles" :key="p.id" :value="p.id">
                {{ p.name }} ({{ p.model === "open_loop" ? `${p.qps} QPS` : "闭环" }} · {{ p.concurrency }} 并发)
              </option>
            </select>
          </div>
        </div>

        <!-- Single Request Summary OR Collection Weighted Request Selector -->
        <div
          v-if="runnerContext.mode === 'single' && runnerContext.draft"
          class="flex items-center justify-between px-3 py-2.5 rounded border border-dividerLight bg-primary"
        >
          <div class="flex items-center space-x-2.5 min-w-0">
            <span
              class="px-2 py-0.5 rounded text-tiny font-bold uppercase border border-divider"
              :class="
                runnerContext.draft.protocol === 'mazbri'
                  ? 'text-accent border-accent/50 bg-accent/10'
                  : 'text-sky-400'
              "
            >
              {{ runnerContext.draft.protocol }}
            </span>
            <span
              class="font-mono font-bold text-tiny"
              :class="methodColorClass(runnerContext.draft.method)"
            >
              {{ runnerContext.draft.method }}
            </span>
            <span class="font-mono text-secondaryDark truncate">
              {{ runnerContext.draft.endpoint }}
            </span>
          </div>
          <span class="text-tiny text-secondaryLight flex-shrink-0 ml-2">
            {{ runnerContext.draft.id ? "已保存请求" : "当前标签页草稿直压" }}
          </span>
        </div>

        <div
          v-else-if="runnerContext.mode === 'collection'"
          class="rounded border border-dividerLight overflow-hidden"
        >
          <div class="flex items-center justify-between px-3 py-2 bg-primaryLight border-b border-dividerLight">
            <span class="font-semibold text-secondaryDark text-tiny">
              勾选参与集合混合压测的接口与流量权重 ({{ enabledCount }}/{{ collectionRows.length }})
            </span>
            <div class="flex items-center space-x-2">
              <HoppButtonSecondary label="全选" @click="toggleAll(true)" />
              <HoppButtonSecondary label="全不选" @click="toggleAll(false)" />
            </div>
          </div>

          <div v-if="collectionRows.length === 0" class="p-6 text-center text-secondaryLight text-tiny">
            该集合内暂无已保存请求，请先在集合中创建或保存请求。
          </div>

          <div v-else class="max-h-52 overflow-y-auto divide-y divide-dividerLight">
            <div
              v-for="(item, idx) in collectionRows"
              :key="item.request.id"
              class="flex items-center justify-between px-3 py-2 hover:bg-primaryLight/50"
            >
              <div class="flex items-center space-x-3 min-w-0 flex-1">
                <HoppSmartCheckbox
                  :on="item.enabled"
                  :name="`runner-req-${idx}`"
                  @change="item.enabled = !item.enabled"
                />
                <span
                  class="px-1.5 py-0.5 rounded text-[10px] uppercase font-mono border border-divider"
                >
                  {{ item.request.protocol }}
                </span>
                <span
                  class="font-mono font-bold text-tiny w-14 flex-shrink-0"
                  :class="methodColorClass(item.request.method)"
                >
                  {{ item.request.method }}
                </span>
                <div class="min-w-0 truncate">
                  <span class="text-secondaryDark font-semibold mr-2">
                    {{ item.request.name }}
                  </span>
                  <span class="text-secondaryLight font-mono text-tiny">
                    {{ item.request.endpoint }}
                  </span>
                </div>
              </div>
              <div class="flex items-center space-x-1.5 flex-shrink-0 ml-3">
                <span class="text-tiny text-secondaryLight">权重</span>
                <input
                  v-model.number="item.weight"
                  type="number"
                  min="1"
                  :disabled="!item.enabled"
                  class="input font-mono w-20 !py-1 !px-2 text-right"
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
    </template>

    <template #footer>
      <div class="flex items-center justify-between w-full">
        <span class="text-tiny text-secondaryLight">
          启动后将冻结当前请求/集合/环境快照并打开实时监控大盘
        </span>
        <div class="flex items-center space-x-2">
          <HoppButtonSecondary label="取消" outline @click="runnerModalOpen = false" />
          <HoppButtonPrimary
            :icon="Play"
            :loading="launching"
            label="立即启动压测"
            @click="launchRun"
          />
        </div>
      </div>
    </template>
  </HoppSmartModal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartCheckbox,
  HoppSmartModal,
} from "@hoppscotch/ui";
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
    // Always pass the normalized current draft so even unsaved edits are stress-tested accurately
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
