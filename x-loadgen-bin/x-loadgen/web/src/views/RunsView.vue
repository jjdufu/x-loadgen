<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-5 p-4 sm:p-6 lg:p-8">
      <!-- Top Filter & Shared Search Toolbar -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <!-- Segmented Status Filter -->
        <div class="inline-flex flex-wrap items-center rounded-lg border border-border bg-muted p-1 gap-0.5">
          <button
            v-for="f in filters"
            :key="f.value"
            type="button"
            class="h-7 cursor-pointer rounded-md px-3 text-xs font-medium whitespace-nowrap transition-all"
            :class="
              statusFilter === f.value
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
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
            placeholder="搜索任务名称、目标或 ID..."
            aria-label="搜索压测任务"
            wrapper-class="min-w-0 flex-1 sm:w-64 sm:flex-none"
          />
          <UiButton variant="outline" size="sm" :disabled="loading" @click="loadRuns">
            <RefreshCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" />
            <span>刷新</span>
          </UiButton>
          <UiButton
            variant="default"
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
        <span class="min-w-0 break-words">任务列表加载失败：{{ loadError }}</span>
        <UiButton variant="outline" size="sm" class="shrink-0" @click="loadRuns">重试</UiButton>
      </div>

      <!-- First load / filter switch -->
      <UiCard
        v-if="loading && runs.length === 0"
        class="flex items-center justify-center gap-2 py-20 text-sm text-muted-foreground"
        aria-busy="true"
      >
        <Loader2 class="h-4 w-4 animate-spin" />
        <span>加载中…</span>
      </UiCard>

      <!-- Empty State -->
      <UiCard
        v-else-if="filteredRuns.length === 0"
        class="flex flex-col items-center justify-center gap-3 border-dashed py-20 text-center"
      >
        <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground">
          <Activity class="h-5 w-5" />
        </div>
        <div class="space-y-1">
          <p class="text-sm font-medium text-foreground">
            {{
              searchQuery.trim() || statusFilter
                ? "未找到匹配的压测任务"
                : "暂无压测任务记录"
            }}
          </p>
          <p class="text-xs text-muted-foreground">
            {{
              searchQuery.trim() || statusFilter
                ? "请尝试调整搜索关键词或状态筛选条件。"
                : "在工作台中任意请求或集合上点击「压测」即可启动任务。"
            }}
          </p>
        </div>
      </UiCard>

      <!-- Phones: one card per run instead of a clipped 6-column table -->
      <ul v-else-if="isMobile" class="space-y-3">
        <li v-for="run in filteredRuns" :key="run.id">
          <UiCard class="space-y-3 p-4">
            <RouterLink
              :to="{ name: 'run-detail', params: { id: run.id } }"
              class="block space-y-1 rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <div class="flex items-start justify-between gap-3">
                <span class="min-w-0 break-words font-medium text-foreground">
                  {{ run.title || run.target_id || "压测任务" }}
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
              <dt class="text-muted-foreground">执行模式</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ formatRunMode(run.mode) }} · {{ run.processes }} 进程
              </dd>
              <dt class="text-muted-foreground">速率 / 并发</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ formatTargetRate(run) }} /
                {{ run.current_concurrency ?? "-" }}
              </dd>
              <dt class="text-muted-foreground">启动时间</dt>
              <dd class="text-right font-mono tabular-nums">
                {{ formatTimeMs(run.started_at ?? run.created_at) }}
              </dd>
            </dl>
            <div class="flex justify-end gap-2">
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
                v-else-if="run.status === 'stopping'"
                variant="outline"
                size="sm"
                disabled
              >
                <Loader2 class="h-3.5 w-3.5 animate-spin" />
                <span>停止中</span>
              </UiButton>
              <UiButton
                v-else
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

      <!-- Shadcn Runs Table -->
      <UiCard v-else class="overflow-hidden">
        <div class="overflow-x-auto">
          <table class="w-full border-collapse text-left text-sm">
            <thead>
              <tr class="border-b border-border bg-muted/40 text-xs font-medium text-muted-foreground">
                <th class="px-5 py-3">任务名称 / ID</th>
                <th class="px-4 py-3">状态</th>
                <th class="px-4 py-3">执行模式</th>
                <th class="px-4 py-3">目标速率 / 并发</th>
                <th class="px-4 py-3">启动时间</th>
                <th class="px-5 py-3 text-right">操作</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border">
              <tr
                v-for="run in filteredRuns"
                :key="run.id"
                tabindex="0"
                role="link"
                :aria-label="`查看压测任务 ${run.title || run.target_id || shortRunId(run.id)}`"
                class="cursor-pointer transition-colors hover:bg-muted/35 focus-visible:bg-muted/50 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
                @click="openRunDetail(run.id)"
                @keydown.enter.self="openRunDetail(run.id)"
                @keydown.space.prevent.self="openRunDetail(run.id)"
              >
                <td class="px-5 py-3.5">
                  <div class="font-medium text-foreground">
                    {{ run.title || run.target_id || "压测任务" }}
                  </div>
                  <div class="mt-0.5 font-mono text-xs text-muted-foreground">
                    {{ shortRunId(run.id) }} · {{ formatRunSource(run) }}
                  </div>
                </td>
                <td class="px-4 py-3.5">
                  <UiBadge :variant="runStatusBadgeVariant(run.status)">
                    {{ formatRunStatus(run.status) }}
                  </UiBadge>
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-foreground tabular-nums">
                  {{ formatRunMode(run.mode) }} · {{ run.processes }} 进程
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-foreground tabular-nums">
                  {{ formatTargetRate(run) }} /
                  {{ run.current_concurrency ?? "-" }} 并发
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-muted-foreground tabular-nums">
                  {{ formatTimeMs(run.started_at ?? run.created_at) }}
                </td>
                <td class="px-5 py-3.5 text-right" @click.stop>
                  <div class="inline-flex items-center gap-1.5">
                    <UiButton
                      variant="outline"
                      size="sm"
                      @click="openRunDetail(run.id)"
                    >
                      详情
                    </UiButton>
                    <UiButton
                      v-if="run.status === 'running'"
                      variant="outline"
                      size="icon-sm"
                      title="停止运行"
                      aria-label="停止运行"
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
                      title="删除任务"
                      aria-label="删除任务"
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
import { Activity, Globe, Loader2, RefreshCw, Square, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { RunView } from "../api/types";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSearchInput from "../components/ui/UiSearchInput.vue";
import {
  formatRunMode,
  formatRunStatus,
  formatTimeMs,
  runStatusBadgeVariant,
} from "../helpers/format";
import { isPageVisible } from "../modules/activity";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";
import { isMobile } from "../modules/viewport";

const router = useRouter();
const route = useRoute();
const runs = ref<RunView[]>([]);
const searchQuery = ref("");
const loading = ref(false);
const loadError = ref<string | null>(null);
const busyRunId = ref<string | null>(null);
let pollTimer: number | null = null;

/** First UUID group (8 hex chars) — never cut a group in the middle. */
function shortRunId(id: string): string {
  return id.split("-")[0] ?? id;
}

function formatRunSource(run: RunView): string {
  if (!run.template_id || run.template_id === "adhoc") {
    return "工作台直发";
  }
  return run.template_id;
}

function formatTargetRate(run: RunView): string {
  if (run.current_qps && run.current_qps > 0) {
    return `${run.current_qps} QPS`;
  }
  if (run.model === "closed_loop") {
    return "闭环饱和";
  }
  if (run.model === "open_loop") {
    return "开环不限速";
  }
  if (run.status === "running" || run.status === "stopping") {
    return "闭环饱和";
  }
  return "-";
}

function openRunDetail(id: string): void {
  void router.push({ name: "run-detail", params: { id } });
}

/** Each tab maps to every backend status it should show. */
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

/** The active tab is kept in `?status=` so reload / back keep it. */
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
      r.template_id.toLowerCase().includes(q),
  );
});

let requestSeq = 0;
let inFlight: Promise<void> | null = null;

/**
 * Fetch the list for the current filter. Every call gets a sequence number
 * and only the newest response is applied, so a slow poll for the previous
 * filter can never overwrite the list of the filter the user just picked.
 * Requests are `silent`: failures show the inline banner instead of a toast
 * every 2.5s while the server is down.
 */
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

/** Background refresh: skipped while hidden or while a request is pending. */
function pollRuns(): void {
  if (!isPageVisible.value || inFlight) return;
  void loadRuns();
}

function selectFilter(val: FilterValue): void {
  if (val === statusFilter.value) return;
  void router.replace({ query: { ...route.query, status: val || undefined } });
}

watch(statusFilter, () => {
  // Don't show the previous filter's rows under the new tab while loading.
  runs.value = [];
  void loadRuns();
});

async function stopRun(id: string): Promise<void> {
  if (busyRunId.value) return;
  const confirmed = await askConfirm({
    title: "停止压测任务",
    description: "确定停止该正在运行的压测任务吗？停止后可随时断点续跑。",
    confirmText: "停止",
    variant: "destructive",
  });
  if (!confirmed) return;
  busyRunId.value = id;
  try {
    await api.stopRun(id);
    toast.info("已发送停止压测信号");
    await loadRuns();
  } finally {
    busyRunId.value = null;
  }
}

async function removeRun(id: string): Promise<void> {
  if (busyRunId.value) return;
  const confirmed = await askConfirm({
    title: "删除压测任务",
    description: "确定删除此压测任务及全部明细样本吗？删除后无法恢复。",
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  busyRunId.value = id;
  try {
    await api.deleteRun(id);
    toast.success("任务已删除");
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
