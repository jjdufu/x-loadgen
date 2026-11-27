<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-5 p-4 sm:p-6 lg:p-8">
      <!-- Top Filter & Shared Search Toolbar -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="inline-flex flex-wrap items-center gap-0.5 rounded-lg border border-border bg-muted p-1">
          <button
            v-for="f in filters"
            :key="f.value"
            type="button"
            class="h-7 cursor-pointer rounded-md px-3 text-xs font-medium whitespace-nowrap transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
            :class="
              statusFilter === f.value
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'
            "
            :aria-pressed="statusFilter === f.value"
            @click="selectFilter(f.value)"
          >
            {{ f.label }}
          </button>
        </div>

        <div class="flex w-full flex-wrap items-center gap-2 sm:w-auto">
          <UiSearchInput
            v-model="searchQuery"
            placeholder="名称、目标、环境或 ID"
            aria-label="搜索压测报告"
            wrapper-class="min-w-0 flex-1 sm:w-64 sm:flex-none"
          />
          <UiButton
            v-if="selectedCompareIds.length > 0"
            variant="default"
            size="sm"
            :disabled="selectedCompareIds.length !== 2"
            @click="openCompare"
          >
            <GitCompare class="h-3.5 w-3.5" />
            <span>对比报告 ({{ selectedCompareIds.length }}/2)</span>
          </UiButton>
          <UiButton variant="outline" size="sm" :disabled="loading" @click="loadRuns">
            <RefreshCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" />
            <span>刷新</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            @click="router.push({ name: 'workspace' })"
          >
            <Globe class="h-3.5 w-3.5" />
            <span>工作台</span>
          </UiButton>
        </div>
      </div>

      <div
        v-if="loadError"
        role="alert"
        class="flex items-center justify-between gap-3 rounded-lg border border-rose-300 bg-rose-50 px-4 py-2.5 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-300"
      >
        <span class="min-w-0 break-words">加载失败：{{ loadError }}</span>
        <UiButton variant="outline" size="sm" class="shrink-0" @click="loadRuns">重试</UiButton>
      </div>

      <UiCard
        v-if="loading && runs.length === 0"
        class="flex items-center justify-center gap-2 py-20 text-sm text-muted-foreground"
        aria-busy="true"
      >
        <Loader2 class="h-4 w-4 animate-spin" />
        <span>加载中…</span>
      </UiCard>

      <UiCard
        v-else-if="filteredRuns.length === 0"
        class="flex flex-col items-center justify-center gap-3 border-dashed py-20 text-center"
      >
        <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground">
          <Activity class="h-5 w-5" />
        </div>
        <p class="text-sm font-medium text-foreground">
          {{ searchQuery.trim() || statusFilter ? "无匹配报告" : "暂无压测报告" }}
        </p>
        <UiButton
          v-if="!searchQuery.trim() && !statusFilter"
          variant="outline"
          size="sm"
          @click="router.push({ name: 'workspace' })"
        >
          工作台
        </UiButton>
      </UiCard>

      <!-- Mobile Cards -->
      <ul v-else-if="isMobile" class="space-y-3">
        <li v-for="run in filteredRuns" :key="run.id">
          <UiCard class="space-y-3 p-4">
            <RouterLink
              :to="{ name: 'run-detail', params: { id: run.id } }"
              class="block space-y-1 rounded-sm"
            >
              <div class="flex items-start justify-between gap-3">
                <span class="min-w-0 break-words font-medium text-foreground">
                  {{ run.title || run.target_id || "压测报告" }}
                </span>
                <UiBadge class="shrink-0" :variant="runStatusBadgeVariant(run.status)">
                  {{ formatRunStatus(run.status) }}
                </UiBadge>
              </div>
              <div class="font-mono text-xs text-muted-foreground">
                {{ shortRunId(run.id) }}
              </div>
            </RouterLink>
            <dl class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs">
              <dt class="text-muted-foreground">样本 / 失败</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ run.samples ?? 0 }} / {{ run.failure ?? 0 }}
              </dd>
              <dt class="text-muted-foreground">实测 QPS / P95</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ run.achieved_qps != null ? `${run.achieved_qps.toFixed(1)}` : "-" }} /
                {{ formatLatencyUs(run.p95_us) }}
              </dd>
              <dt class="text-muted-foreground">时间</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ formatTimeMs(run.started_at ?? run.created_at) }}
              </dd>
            </dl>
            <div class="flex justify-end gap-2">
              <UiButton
                variant="outline"
                size="sm"
                :loading="replayingId === run.id"
                @click="rerunTask(run.id)"
              >
                <RotateCcw v-if="replayingId !== run.id" class="h-3.5 w-3.5" />
                <span>重跑</span>
              </UiButton>
              <UiButton
                v-if="run.status === 'running'"
                variant="outline"
                size="sm"
                :loading="busyRunId === run.id"
                class="text-rose-600 dark:text-rose-400"
                @click="stopRun(run.id)"
              >
                <Square v-if="busyRunId !== run.id" class="h-3.5 w-3.5" />
                <span>停止</span>
              </UiButton>
              <UiButton
                v-else-if="run.status !== 'stopping'"
                variant="outline"
                size="sm"
                :loading="busyRunId === run.id"
                @click="removeRun(run.id)"
              >
                <Trash2 v-if="busyRunId !== run.id" class="h-3.5 w-3.5" />
                <span>删除</span>
              </UiButton>
            </div>
          </UiCard>
        </li>
      </ul>

      <!-- Desktop Table -->
      <UiCard v-else class="overflow-hidden">
        <div class="overflow-x-auto">
          <table class="w-full border-collapse text-left text-sm">
            <thead>
              <tr class="border-b border-border bg-muted/40 text-xs font-medium text-muted-foreground">
                <th class="w-10 py-3 pl-4 pr-2">
                  <span class="sr-only">对比</span>
                </th>
                <th class="px-4 py-3">报告名称 / 形态</th>
                <th class="px-3 py-3">状态 / SLA</th>
                <th class="px-3 py-3">样本 / 错误率</th>
                <th class="px-3 py-3">实测 QPS / 目标</th>
                <th class="px-3 py-3">P95 / P99</th>
                <th class="px-3 py-3">启动时间</th>
                <th class="px-5 py-3 text-right">操作</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border">
              <tr
                v-for="run in filteredRuns"
                :key="run.id"
                tabindex="0"
                role="link"
                :aria-label="`查看压测报告 ${run.title || shortRunId(run.id)}`"
                class="cursor-pointer transition-colors hover:bg-accent-soft-hover"
                @click="openRunDetail(run.id)"
                @keydown.enter.self="openRunDetail(run.id)"
              >
                <td class="py-3.5 pl-4 pr-2" @click.stop>
                  <input
                    type="checkbox"
                    :checked="selectedCompareIds.includes(run.id)"
                    :disabled="selectedCompareIds.length >= 2 && !selectedCompareIds.includes(run.id)"
                    :aria-label="`选择对比 ${run.title || shortRunId(run.id)}`"
                    class="h-4 w-4 cursor-pointer rounded border-input accent-foreground"
                    @change="toggleCompare(run.id)"
                  />
                </td>
                <td class="px-4 py-3.5">
                  <div class="flex items-center gap-1.5">
                    <span class="font-medium text-foreground">
                      {{ run.title || run.target_id || "压测报告" }}
                    </span>
                    <span
                      v-for="p in run.protocols ?? []"
                      :key="p"
                      class="rounded border border-border bg-muted px-1 py-0.5 font-mono text-[10px] uppercase text-muted-foreground"
                    >
                      {{ p }}
                    </span>
                  </div>
                  <div class="mt-0.5 font-mono text-xs text-muted-foreground">
                    {{ shortRunId(run.id) }} · {{ formatRunMode(run.mode) }} · {{ run.processes }} 进程
                    <template v-if="run.environment_name"> · {{ run.environment_name }}</template>
                  </div>
                </td>
                <td class="px-3 py-3.5">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <UiBadge :variant="runStatusBadgeVariant(run.status)">
                      {{ formatRunStatus(run.status) }}
                    </UiBadge>
                    <UiBadge
                      v-if="run.verdict_result"
                      :variant="run.verdict_result === 'pass' ? 'success' : run.verdict_result === 'fail' ? 'destructive' : 'warning'"
                    >
                      {{ formatVerdictResult(run.verdict_result) }}
                    </UiBadge>
                  </div>
                </td>
                <td class="px-3 py-3.5 font-mono text-xs tabular-nums">
                  <div class="text-foreground">{{ run.samples ?? 0 }}</div>
                  <div
                    :class="
                      (run.failure ?? 0) > 0
                        ? 'text-rose-600 dark:text-rose-400'
                        : 'text-emerald-600 dark:text-emerald-400'
                    "
                  >
                    {{ formatErrorRate(run) }}
                  </div>
                </td>
                <td class="px-3 py-3.5 font-mono text-xs tabular-nums">
                  <div class="text-foreground">
                    {{ run.achieved_qps != null ? `${run.achieved_qps.toFixed(1)} req/s` : "-" }}
                  </div>
                  <div class="text-muted-foreground">
                    {{ formatTargetRate(run) }} / {{ run.current_concurrency ?? "-" }}并发
                  </div>
                </td>
                <td class="px-3 py-3.5 font-mono text-xs tabular-nums">
                  <div class="text-foreground">{{ formatLatencyUs(run.p95_us) }}</div>
                  <div class="text-muted-foreground">{{ formatLatencyUs(run.p99_us) }}</div>
                </td>
                <td class="px-3 py-3.5 font-mono text-xs text-muted-foreground tabular-nums">
                  {{ formatTimeMs(run.started_at ?? run.created_at) }}
                </td>
                <td class="px-5 py-3.5 text-right" @click.stop>
                  <div class="inline-flex items-center gap-1.5">
                    <UiButton
                      variant="outline"
                      size="sm"
                      :loading="replayingId === run.id"
                      @click="rerunTask(run.id)"
                    >
                      <RotateCcw v-if="replayingId !== run.id" class="h-3.5 w-3.5" />
                      <span>重跑</span>
                    </UiButton>
                    <UiButton variant="outline" size="sm" @click="openRunDetail(run.id)">
                      报告
                    </UiButton>
                    <UiButton
                      v-if="run.status === 'running'"
                      variant="outline"
                      size="icon-sm"
                      title="停止"
                      aria-label="停止"
                      :loading="busyRunId === run.id"
                      class="text-rose-600 dark:text-rose-400"
                      @click="stopRun(run.id)"
                    >
                      <Square v-if="busyRunId !== run.id" class="h-3.5 w-3.5" />
                    </UiButton>
                    <UiButton
                      v-else-if="run.status === 'stopping'"
                      variant="ghost"
                      size="icon-sm"
                      title="停止中"
                      aria-label="停止中"
                      disabled
                    >
                      <Loader2 class="h-3.5 w-3.5 animate-spin" />
                    </UiButton>
                    <UiButton
                      v-else
                      variant="ghost"
                      size="icon-sm"
                      title="删除"
                      aria-label="删除"
                      :loading="busyRunId === run.id"
                      class="hover:text-rose-600 dark:hover:text-rose-400"
                      @click="removeRun(run.id)"
                    >
                      <Trash2 v-if="busyRunId !== run.id" class="h-3.5 w-3.5" />
                    </UiButton>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Activity,
  GitCompare,
  Globe,
  Loader2,
  RefreshCw,
  RotateCcw,
  Square,
  Trash2,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import type { RunView } from "../api/types";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSearchInput from "../components/ui/UiSearchInput.vue";
import {
  formatLatencyUs,
  formatRunMode,
  formatRunStatus,
  formatTimeMs,
  formatVerdictResult,
  runStatusBadgeVariant,
} from "../helpers/format";
import { isPageVisible } from "../modules/activity";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";
import { isMobile } from "../modules/viewport";
import { openRunReplayInWorkspace } from "../stores/workspace";

const router = useRouter();
const route = useRoute();
const runs = ref<RunView[]>([]);
const searchQuery = ref("");
const loading = ref(false);
const loadError = ref<string | null>(null);
const busyRunId = ref<string | null>(null);
const replayingId = ref<string | null>(null);
const selectedCompareIds = ref<string[]>([]);
let pollTimer: number | null = null;

function shortRunId(id: string): string {
  return id.split("-")[0] ?? id;
}

function formatErrorRate(run: RunView): string {
  const s = run.samples ?? 0;
  const f = run.failure ?? 0;
  if (s <= 0) return "0.00%";
  return `${((f / s) * 100).toFixed(2)}%`;
}

function formatTargetRate(run: RunView): string {
  if (run.current_qps && run.current_qps > 0) return `${run.current_qps} QPS`;
  if (run.model === "closed_loop") return "闭环";
  if (run.model === "open_loop") return "不限速";
  return "-";
}

function openRunDetail(id: string): void {
  void router.push({ name: "run-detail", params: { id } });
}

function toggleCompare(id: string): void {
  const idx = selectedCompareIds.value.indexOf(id);
  if (idx >= 0) {
    selectedCompareIds.value.splice(idx, 1);
  } else if (selectedCompareIds.value.length < 2) {
    selectedCompareIds.value.push(id);
  }
}

function openCompare(): void {
  if (selectedCompareIds.value.length !== 2) return;
  const [first, second] = selectedCompareIds.value;
  void router.push({
    name: "run-compare",
    query: { base: second, target: first },
  });
}

async function rerunTask(id: string): Promise<void> {
  if (replayingId.value) return;
  replayingId.value = id;
  try {
    const replay = await api.getRunReplay(id);
    openRunReplayInWorkspace(replay);
    await router.push({ name: "workspace" });
    toast.success("已载入历史配置");
  } finally {
    replayingId.value = null;
  }
}

const filters = [
  { value: "", label: "全部", statuses: "" },
  { value: "running", label: "运行中", statuses: "running" },
  { value: "stopping", label: "停止中", statuses: "stopping" },
  { value: "finished", label: "已完成", statuses: "finished" },
  { value: "stopped", label: "已停止", statuses: "stopped" },
  { value: "interrupted", label: "已中断", statuses: "interrupted" },
  { value: "failed", label: "失败", statuses: "failed" },
] as const;
type FilterValue = (typeof filters)[number]["value"];

const statusFilter = computed<FilterValue>(() => {
  const raw = route.query.status;
  return filters.find((f) => f.value === raw)?.value ?? "";
});

const filteredRuns = computed<RunView[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return runs.value;
  return runs.value.filter(
    (r) =>
      (r.title && r.title.toLowerCase().includes(q)) ||
      r.id.toLowerCase().includes(q) ||
      (r.target_id && r.target_id.toLowerCase().includes(q)) ||
      (r.environment_name && r.environment_name.toLowerCase().includes(q)) ||
      r.template_id.toLowerCase().includes(q),
  );
});

let requestSeq = 0;
let inFlight: Promise<void> | null = null;

function loadRuns(): Promise<void> {
  const seq = ++requestSeq;
  const statuses = filters.find((f) => f.value === statusFilter.value)?.statuses;
  loading.value = true;
  const p = api
    .listRuns(statuses || undefined, { silent: true })
    .then((list) => {
      if (seq !== requestSeq) return;
      runs.value = list;
      loadError.value = null;
    })
    .catch((err: unknown) => {
      if (seq !== requestSeq) return;
      loadError.value = err instanceof Error ? err.message : String(err);
    })
    .finally(() => {
      if (seq === requestSeq) loading.value = false;
      if (inFlight === p) inFlight = null;
    });
  inFlight = p;
  return p;
}

function pollRuns(): void {
  if (!isPageVisible.value || inFlight) return;
  void loadRuns();
}

function selectFilter(val: FilterValue): void {
  if (val === statusFilter.value) return;
  void router.replace({ query: { ...route.query, status: val || undefined } });
}

watch(statusFilter, () => {
  runs.value = [];
  void loadRuns();
});

async function stopRun(id: string): Promise<void> {
  if (busyRunId.value) return;
  const confirmed = await askConfirm({
    title: "停止压测",
    description: "停止后任务将进入可续跑状态。",
    confirmText: "停止",
    variant: "destructive",
  });
  if (!confirmed) return;
  busyRunId.value = id;
  try {
    await api.stopRun(id);
    toast.info("已发送停止信号");
    await loadRuns();
  } finally {
    busyRunId.value = null;
  }
}

async function removeRun(id: string): Promise<void> {
  if (busyRunId.value) return;
  const confirmed = await askConfirm({
    title: "删除报告",
    description: "将删除此压测报告及全部明细样本，删除后无法恢复。",
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  busyRunId.value = id;
  try {
    await api.deleteRun(id);
    selectedCompareIds.value = selectedCompareIds.value.filter((x) => x !== id);
    toast.success("已删除");
    await loadRuns();
  } finally {
    busyRunId.value = null;
  }
}

watch(isPageVisible, (visible) => {
  if (visible) pollRuns();
});

onMounted(() => {
  void loadRuns();
  pollTimer = window.setInterval(pollRuns, 2500);
});

onBeforeUnmount(() => {
  requestSeq++;
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
