<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-6 lg:p-8">
      <!-- Top Header & Controls -->
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="space-y-1.5">
          <div class="flex flex-wrap items-center gap-2.5">
            <UiButton
              variant="outline"
              size="icon-sm"
              aria-label="返回压测任务列表"
              @click="router.push({ name: 'runs' })"
            >
              <ArrowLeft class="h-4 w-4" />
            </UiButton>
            <h1 class="text-xl font-semibold tracking-tight text-foreground">
              {{ run?.title || "压测任务详情" }}
            </h1>
            <UiBadge
              v-if="run"
              :variant="isRunning ? 'success' : 'secondary'"
            >
              {{ formatRunStatus(run.status) }}
            </UiBadge>
            <UiBadge
              v-if="run?.verdict && run.verdict.result !== 'none'"
              :variant="verdictVariant(run.verdict.result)"
            >
              SLA: {{ formatVerdictResult(run.verdict.result) }}
            </UiBadge>
          </div>
          <p class="pl-10 font-mono text-xs text-muted-foreground tabular-nums">
            ID: {{ id }} · 模式: {{ formatRunMode(run?.mode) }} · 工作进程: {{ run?.processes ?? 1 }}
          </p>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <UiButton
            v-if="isRunning"
            variant="destructive"
            size="sm"
            @click="handleStop"
          >
            <Square class="h-3.5 w-3.5" />
            <span>停止</span>
          </UiButton>
          <UiButton
            v-if="canResume"
            variant="default"
            size="sm"
            @click="handleResume"
          >
            <Play class="h-3.5 w-3.5" />
            <span>续跑</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            @click="downloadReport('json')"
          >
            <Download class="h-3.5 w-3.5" />
            <span>JSON 报告</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            @click="downloadReport('html')"
          >
            <FileCode2 class="h-3.5 w-3.5" />
            <span>HTML 报告</span>
          </UiButton>
        </div>
      </div>

      <!-- Error Banner -->
      <div
        v-if="run?.error"
        class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-4 py-3 font-mono text-xs text-rose-700 dark:text-rose-400"
      >
        任务错误：{{ run.error }}
      </div>

      <!-- Shadcn Dashboard KPI Metric Cards -->
      <div class="grid grid-cols-2 gap-3.5 sm:grid-cols-4 lg:grid-cols-8">
        <UiCard
          v-for="card in metricCards"
          :key="card.label"
          class="p-4 space-y-1.5"
        >
          <div class="text-xs font-medium text-muted-foreground whitespace-nowrap">
            {{ card.label }}
          </div>
          <div
            class="font-mono text-xl font-semibold tracking-tight tabular-nums"
            :class="card.colorClass ?? 'text-foreground'"
          >
            {{ card.value }}
          </div>
        </UiCard>
      </div>

      <!-- Live Traffic Tuner Card -->
      <UiCard class="p-4">
        <form
          class="flex flex-wrap items-end gap-4"
          @submit.prevent="applyTrafficPatch"
        >
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              动态目标 QPS
            </label>
            <UiInput
              :model-value="tuneQps"
              type="number"
              min="1"
              mono
              :disabled="!isRunning"
              class="w-36"
              @update:model-value="(v) => (tuneQps = Math.max(1, Number(v) || 1))"
            />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              动态并发上限
            </label>
            <UiInput
              :model-value="tuneConcurrency"
              type="number"
              min="1"
              mono
              :disabled="!isRunning"
              class="w-36"
              @update:model-value="(v) => (tuneConcurrency = Math.max(1, Number(v) || 1))"
            />
          </div>
          <UiButton
            variant="default"
            :disabled="!isRunning"
            @click="applyTrafficPatch"
          >
            应用
          </UiButton>
          <span class="pb-2 text-xs text-muted-foreground">
            运行期间可随时调整 QPS 与并发，实时同步至全部工作进程。
          </span>
        </form>
      </UiCard>

      <!-- ECharts Live Throughput Series -->
      <UiCard class="p-5 space-y-3">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-sm font-semibold text-foreground">
              每秒吞吐量与成功 / 失败趋势
            </h2>
            <p class="text-xs text-muted-foreground">
              按秒聚合全集群样本速率
            </p>
          </div>
        </div>
        <div ref="chartEl" class="h-72 w-full" />
      </UiCard>

      <!-- Bottom Detailed Explorer Card -->
      <UiCard class="overflow-hidden">
        <div
          class="flex h-11 items-center gap-1 overflow-x-auto border-b border-border bg-surface/50 px-4 select-none"
          role="tablist"
        >
          <button
            v-for="tab in detailTabs"
            :key="tab.id"
            type="button"
            role="tab"
            :aria-selected="activeDetailTab === tab.id"
            class="relative inline-flex h-full items-center gap-1.5 px-3 text-xs font-medium whitespace-nowrap transition-colors"
            :class="
              activeDetailTab === tab.id
                ? 'text-foreground'
                : 'text-muted-foreground hover:text-foreground'
            "
            @click="activeDetailTab = tab.id"
          >
            <span>{{ tab.label }}</span>
            <span
              v-if="tab.count"
              class="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-foreground tabular-nums"
            >
              {{ tab.count }}
            </span>
            <span
              v-if="activeDetailTab === tab.id"
              class="absolute inset-x-2 bottom-0 h-[2px] rounded-full bg-foreground"
            />
          </button>
        </div>

        <!-- 1. Samples Explorer -->
        <div v-if="activeDetailTab === 'samples'" class="p-5 space-y-4">
          <div class="flex flex-wrap items-end gap-3 border-b border-border pb-4">
            <div class="space-y-1">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                结果状态
              </label>
              <UiSelect v-model="sampleFilterSuccess" size="sm" wrapper-class="w-36">
                <option value="all">全部样本</option>
                <option value="false">仅失败请求</option>
                <option value="true">仅成功请求</option>
              </UiSelect>
            </div>
            <div class="space-y-1">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                错误分类
              </label>
              <UiInput
                v-model="sampleFilterErrClass"
                size="sm"
                mono
                placeholder="如 http_5xx / timeout"
                class="w-52"
              />
            </div>
            <div class="space-y-1">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                最低延迟 (ms)
              </label>
              <UiInput
                v-model="sampleFilterMinMs"
                type="number"
                min="0"
                size="sm"
                mono
                placeholder="0"
                class="w-28"
              />
            </div>
            <UiButton variant="outline" size="sm" @click="loadSamples(true)">
              检索
            </UiButton>
          </div>

          <div
            v-if="samples.length === 0"
            class="py-10 text-center text-sm text-muted-foreground"
          >
            暂无匹配的请求明细样本
          </div>

          <div v-else class="overflow-x-auto">
            <table class="w-full border-collapse text-left font-mono text-xs">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="py-2.5 pr-4 font-medium">进程#序号</th>
                  <th class="py-2.5 pr-4 font-medium">接口</th>
                  <th class="py-2.5 pr-4 font-medium">状态</th>
                  <th class="py-2.5 pr-4 font-medium">延迟</th>
                  <th class="py-2.5 pr-4 font-medium">字节</th>
                  <th class="py-2.5 font-medium">错误 / 诊断信息</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr
                  v-for="s in samples"
                  :key="`${s.worker}-${s.seq}`"
                  class="hover:bg-muted/35"
                >
                  <td class="py-2 pr-4 text-muted-foreground tabular-nums">
                    w{{ s.worker }}#{{ s.seq }}
                  </td>
                  <td class="py-2 pr-4 text-foreground">
                    {{ s.product_name || s.product_id }}
                  </td>
                  <td class="py-2 pr-4">
                    <span
                      class="font-semibold"
                      :class="
                        s.success
                          ? 'text-emerald-600 dark:text-emerald-400'
                          : 'text-rose-600 dark:text-rose-400'
                      "
                    >
                      {{ s.http_status ?? (s.success ? "成功" : "失败") }}
                    </span>
                  </td>
                  <td class="py-2 pr-4 text-foreground tabular-nums">
                    {{ formatLatencyUs(s.latency_us) }}
                  </td>
                  <td class="py-2 pr-4 text-muted-foreground tabular-nums">
                    {{ formatBytes(s.response_bytes) }}
                  </td>
                  <td class="max-w-md truncate py-2 text-rose-600 dark:text-rose-400">
                    {{ s.error_class ? `[${s.error_class}] ` : "" }}{{ s.error ?? "" }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="sampleNextCursor" class="flex justify-center pt-2">
            <UiButton variant="outline" size="sm" @click="loadSamples(false)">
              加载更多
            </UiButton>
          </div>
        </div>

        <!-- 2. SLA Verdict -->
        <div v-else-if="activeDetailTab === 'verdict'" class="p-5 space-y-4 text-sm">
          <div
            v-if="!run?.verdict || run.verdict.result === 'none'"
            class="py-6 text-muted-foreground"
          >
            当前压测未配置 SLA 阈值判定规则。
          </div>
          <template v-else>
            <div class="flex items-center gap-2 font-medium text-foreground">
              <span>综合结论：</span>
              <UiBadge :variant="verdictVariant(run.verdict.result)">
                {{ formatVerdictResult(run.verdict.result) }}
              </UiBadge>
            </div>
            <div v-if="run.verdict.failed?.length" class="space-y-2">
              <div
                v-for="fc in run.verdict.failed"
                :key="fc.check"
                class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-4 py-2.5 font-mono text-xs text-rose-700 dark:text-rose-400"
              >
                未达标项 {{ fc.check }}：目标阈值 {{ fc.wanted }}，实测值 {{ fc.actual ?? "-" }}
              </div>
            </div>
            <pre
              v-if="run.verdict.actual"
              class="rounded-lg border border-border bg-muted/40 p-4 font-mono text-xs text-foreground"
            >{{ JSON.stringify(run.verdict.actual, null, 2) }}</pre>
          </template>
        </div>

        <!-- 3. Loop Windows -->
        <div v-else-if="activeDetailTab === 'windows'" class="overflow-x-auto p-5">
          <div
            v-if="windows.length === 0"
            class="py-6 text-center text-sm text-muted-foreground"
          >
            当前任务无多轮循环窗口记录
          </div>
          <table v-else class="w-full border-collapse text-left font-mono text-xs tabular-nums">
            <thead>
              <tr class="border-b border-border text-muted-foreground">
                <th class="py-2.5 pr-4 font-medium">轮次 #</th>
                <th class="py-2.5 pr-4 font-medium">样本数</th>
                <th class="py-2.5 pr-4 font-medium">成功</th>
                <th class="py-2.5 pr-4 font-medium">失败</th>
                <th class="py-2.5 pr-4 font-medium">p50</th>
                <th class="py-2.5 pr-4 font-medium">p95</th>
                <th class="py-2.5 font-medium">p99</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border">
              <tr v-for="w in windows" :key="w.window_index">
                <td class="py-2 pr-4 text-foreground">#{{ w.window_index }}</td>
                <td class="py-2 pr-4 text-foreground">{{ w.samples }}</td>
                <td class="py-2 pr-4 text-emerald-600 dark:text-emerald-400">
                  {{ w.success }}
                </td>
                <td class="py-2 pr-4 text-rose-600 dark:text-rose-400">
                  {{ w.failure }}
                </td>
                <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p50_us) }}</td>
                <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p95_us) }}</td>
                <td class="py-2 text-foreground">{{ formatLatencyUs(w.p99_us) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- 4. Lifecycle Timeline -->
        <div
          v-else-if="activeDetailTab === 'timeline'"
          class="divide-y divide-border p-5 font-mono text-xs"
        >
          <div
            v-for="ev in timeline"
            :key="ev.id"
            class="flex flex-wrap items-center justify-between gap-2 py-2.5"
          >
            <div>
              <span class="mr-3 font-semibold text-foreground">{{ ev.event }}</span>
              <span v-if="ev.payload" class="text-muted-foreground">
                {{ JSON.stringify(ev.payload) }}
              </span>
            </div>
            <span class="text-muted-foreground tabular-nums">
              {{ formatTimeMs(ev.ts) }}
            </span>
          </div>
        </div>

        <!-- 5. Redacted Snapshot -->
        <div v-else class="p-5">
          <CodeMirrorEditor
            :model-value="JSON.stringify(run?.snapshot ?? {}, null, 2)"
            language="json"
            readonly
            min-height="220px"
            max-height="480px"
          />
        </div>
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import * as echarts from "echarts";
import {
  ArrowLeft,
  Download,
  FileCode2,
  Play,
  Square,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import type {
  RunStats,
  RunView,
  SampleRow,
  TimePoint,
  TimelineEvent,
  WindowRow,
} from "../api/types";
import CodeMirrorEditor from "../components/common/CodeMirrorEditor.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import {
  formatBytes,
  formatLatencyUs,
  formatRunMode,
  formatRunStatus,
  formatTimeMs,
  formatVerdictResult,
} from "../helpers/format";
import { effectiveTheme } from "../modules/theme";
import { toast } from "../modules/toast";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id));

const run = ref<RunView | null>(null);
const stats = ref<RunStats | null>(null);
const windows = ref<WindowRow[]>([]);
const timeline = ref<TimelineEvent[]>([]);
const samples = ref<SampleRow[]>([]);
const lastSeries = ref<TimePoint[]>([]);
const sampleNextCursor = ref<{
  after_worker?: number;
  after_seq?: number;
  worker?: number;
  seq?: number;
} | null>(null);

const sampleFilterSuccess = ref<"all" | "true" | "false">("all");
const sampleFilterErrClass = ref("");
const sampleFilterMinMs = ref("");

const tuneQps = ref(20);
const tuneConcurrency = ref(10);
const activeDetailTab = ref<"samples" | "verdict" | "windows" | "timeline" | "snapshot">("samples");

const chartEl = ref<HTMLDivElement | null>(null);
let chart: echarts.ECharts | null = null;
let sse: EventSource | null = null;
let pollTimer: number | null = null;
let initializedInputs = false;

const isRunning = computed(
  () => run.value?.status === "running" || run.value?.status === "stopping",
);
const canResume = computed(
  () => run.value?.status === "stopped" || run.value?.status === "interrupted",
);

const detailTabs = computed(() => [
  { id: "samples" as const, label: "样本明细", count: samples.value.length },
  { id: "verdict" as const, label: "SLA 判定", count: 0 },
  { id: "windows" as const, label: "循环窗口", count: windows.value.length },
  { id: "timeline" as const, label: "事件时间线", count: timeline.value.length },
  { id: "snapshot" as const, label: "执行快照", count: 0 },
]);

const metricCards = computed(() => {
  const s = stats.value;
  return [
    { label: "样本总数", value: s?.samples ?? 0 },
    {
      label: "成功请求",
      value: s?.success ?? 0,
      colorClass: "text-emerald-600 dark:text-emerald-400",
    },
    {
      label: "失败请求",
      value: s?.failure ?? 0,
      colorClass:
        (s?.failure ?? 0) > 0
          ? "text-rose-600 dark:text-rose-400"
          : "text-foreground",
    },
    { label: "p50 延迟", value: formatLatencyUs(s?.p50_us) },
    { label: "p95 延迟", value: formatLatencyUs(s?.p95_us) },
    { label: "p99 延迟", value: formatLatencyUs(s?.p99_us) },
    { label: "在途并发", value: s?.in_flight ?? 0 },
    { label: "累计字节", value: formatBytes(s?.bytes) },
  ];
});

function verdictVariant(result: string): "success" | "destructive" | "warning" {
  if (result === "pass") return "success";
  if (result === "fail") return "destructive";
  return "warning";
}

async function refreshAll(): Promise<void> {
  const [runData, statsData, winData, timeData, seriesData] = await Promise.all([
    api.getRun(id.value),
    api.getRunStats(id.value),
    api.getRunWindows(id.value),
    api.getRunTimeline(id.value),
    api.getRunTimeseries(id.value),
  ]);
  run.value = runData;
  stats.value = statsData;
  windows.value = winData;
  timeline.value = timeData;

  let effectiveSeries = seriesData;
  if (effectiveSeries.length === 0) {
    if (lastSeries.value.length > 1) {
      effectiveSeries = lastSeries.value;
    } else if (statsData.samples > 0) {
      try {
        const report = await api.getRunReport(id.value);
        effectiveSeries = report.timeseries ?? [];
      } catch {
        // Keep empty series if report is still finalizing
      }
    }
  }
  lastSeries.value = effectiveSeries;

  if (!initializedInputs) {
    if (statsData.current_qps) tuneQps.value = statsData.current_qps;
    if (statsData.current_concurrency) {
      tuneConcurrency.value = statsData.current_concurrency;
    }
    initializedInputs = true;
  }
  renderChart(effectiveSeries);
}

async function loadSamples(reset: boolean): Promise<void> {
  const cursor = reset ? null : sampleNextCursor.value;
  const page = await api.getRunSamples(id.value, {
    limit: 50,
    after_worker: cursor?.after_worker ?? cursor?.worker,
    after_seq: cursor?.after_seq ?? cursor?.seq,
    success:
      sampleFilterSuccess.value === "all"
        ? undefined
        : sampleFilterSuccess.value === "true",
    error_class: sampleFilterErrClass.value.trim() || undefined,
    min_latency_us: sampleFilterMinMs.value
      ? Math.round(Number(sampleFilterMinMs.value) * 1000)
      : undefined,
  });
  samples.value = reset ? page.items : [...samples.value, ...page.items];
  sampleNextCursor.value = page.next;
}

function renderChart(points: TimePoint[]): void {
  if (!chartEl.value) return;
  if (!chart) {
    chart = echarts.init(chartEl.value);
  }
  const isDark = effectiveTheme.value === "black";
  const axisColor = isDark ? "#a1a1aa" : "#71717a";
  const gridColor = isDark ? "rgba(255,255,255,0.08)" : "rgba(9,9,11,0.07)";
  const primaryLine = isDark ? "#fafafa" : "#18181b";
  const showDot = points.length <= 2;

  const labels = points.map((p) => {
    const raw = p.unix_sec ?? p.started_at;
    if (!raw) return "";
    const ms = raw > 1e11 ? raw : raw * 1000;
    return new Date(ms).toLocaleTimeString("zh-CN", { hour12: false });
  });

  chart.setOption({
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      backgroundColor: isDark ? "#111114" : "#ffffff",
      borderColor: isDark ? "#27272a" : "#e4e4e7",
      textStyle: {
        color: isDark ? "#fafafa" : "#09090b",
        fontSize: 12,
      },
    },
    grid: { left: 44, right: 20, top: 34, bottom: 26 },
    legend: {
      data: ["总样本/s", "成功/s", "失败/s"],
      textStyle: { color: axisColor, fontSize: 12 },
    },
    xAxis: {
      type: "category",
      data: labels,
      axisLine: { lineStyle: { color: gridColor } },
      axisLabel: { color: axisColor, fontSize: 11 },
    },
    yAxis: {
      type: "value",
      splitLine: { lineStyle: { color: gridColor } },
      axisLabel: { color: axisColor, fontSize: 11 },
    },
    series: [
      {
        name: "总样本/s",
        type: "line",
        smooth: true,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2 },
        areaStyle: {
          color: isDark ? "rgba(250,250,250,0.05)" : "rgba(24,24,27,0.04)",
        },
        data: points.map((p) => p.samples || 0),
        itemStyle: { color: primaryLine },
      },
      {
        name: "成功/s",
        type: "line",
        smooth: true,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2 },
        data: points.map((p) => p.success || 0),
        itemStyle: { color: isDark ? "#34d399" : "#059669" },
      },
      {
        name: "失败/s",
        type: "line",
        smooth: true,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2 },
        data: points.map((p) => p.failure || 0),
        itemStyle: { color: isDark ? "#fb7185" : "#e11d48" },
      },
    ],
  });
}

watch(effectiveTheme, () => {
  renderChart(lastSeries.value);
});

function startLiveStream(): void {
  try {
    sse = new EventSource(`/api/v1/runs/${id.value}/events`);
    sse.addEventListener("stats", () => {
      void refreshAll();
    });
    sse.onerror = () => {
      sse?.close();
      sse = null;
      startFallbackPoll();
    };
  } catch {
    startFallbackPoll();
  }
}

function startFallbackPoll(): void {
  if (pollTimer !== null) return;
  pollTimer = window.setInterval(() => {
    void refreshAll();
  }, 1200);
}

async function handleStop(): Promise<void> {
  await api.stopRun(id.value);
  toast.info("已发送停止信号");
  await refreshAll();
  await loadSamples(true);
}

async function handleResume(): Promise<void> {
  await api.resumeRun(id.value);
  toast.success("已恢复运行");
  await refreshAll();
}

async function applyTrafficPatch(): Promise<void> {
  await api.patchTraffic(id.value, {
    qps: tuneQps.value,
    concurrency: tuneConcurrency.value,
  });
  toast.success("流量参数已实时下发");
  await refreshAll();
}

async function downloadReport(kind: "json" | "html"): Promise<void> {
  const blob = await api.downloadReportBlob(id.value, kind);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `run-${id.value}-report.${kind}`;
  a.click();
  URL.revokeObjectURL(url);
  toast.success(`已导出 ${kind.toUpperCase()} 报告`);
}

function handleResize(): void {
  chart?.resize();
}

onMounted(async () => {
  window.addEventListener("resize", handleResize);
  await Promise.all([refreshAll(), loadSamples(true)]);
  startLiveStream();
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", handleResize);
  sse?.close();
  sse = null;
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
  chart?.dispose();
  chart = null;
});
</script>
