<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-y-auto p-6 space-y-5">
    <!-- Top Header & Controls -->
    <div class="flex flex-wrap items-center justify-between gap-3 border-b border-dividerLight pb-4">
      <div class="space-y-1">
        <div class="flex items-center space-x-2.5">
          <HoppButtonSecondary
            :icon="ArrowLeft"
            outline
            @click="router.push({ name: 'runs' })"
          />
          <h1 class="heading">
            {{ run?.title || "压测任务实时监控" }}
          </h1>
          <span
            v-if="run"
            class="px-2.5 py-0.5 rounded-full font-mono text-tiny border"
            :class="
              isRunning
                ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-400'
                : 'border-divider bg-primaryLight text-secondaryDark'
            "
          >
            {{ run.status }}
          </span>
          <span
            v-if="run?.verdict && run.verdict.result !== 'none'"
            class="px-2.5 py-0.5 rounded-full font-mono text-tiny border"
            :class="verdictClass(run.verdict.result)"
          >
            SLA: {{ run.verdict.result.toUpperCase() }}
          </span>
        </div>
        <p class="text-tiny text-secondaryLight font-mono pl-10">
          ID: {{ id }} · 模式: {{ run?.mode ?? "-" }} · 工作进程: {{ run?.processes ?? 1 }}
        </p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <HoppButtonSecondary
          v-if="isRunning"
          :icon="Square"
          label="停止压测"
          color="red"
          outline
          @click="handleStop"
        />
        <HoppButtonPrimary
          v-if="canResume"
          :icon="Play"
          label="断点续跑"
          @click="handleResume"
        />
        <HoppButtonSecondary
          :icon="Download"
          label="下载 JSON 报告"
          outline
          @click="downloadReport('json')"
        />
        <HoppButtonSecondary
          :icon="FileCode2"
          label="下载独立 HTML 报告"
          outline
          @click="downloadReport('html')"
        />
      </div>
    </div>

    <div
      v-if="run?.error"
      class="px-4 py-2.5 rounded border border-rose-500/40 bg-rose-500/15 text-rose-400 text-tiny font-mono"
    >
      任务错误：{{ run.error }}
    </div>

    <!-- Real-Time Metric Cards -->
    <div class="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-3">
      <div
        v-for="card in metricCards"
        :key="card.label"
        class="p-3 rounded border border-dividerLight bg-primaryLight/40"
      >
        <div class="text-[11px] text-secondaryLight">{{ card.label }}</div>
        <div class="text-lg font-bold font-mono mt-0.5" :class="card.colorClass ?? 'text-secondaryDark'">
          {{ card.value }}
        </div>
      </div>
    </div>

    <!-- Live Traffic Tuner Bar -->
    <form
      class="flex flex-wrap items-end gap-3 p-3 rounded border border-dividerLight bg-primaryLight/30"
      @submit.prevent="applyTrafficPatch"
    >
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          在线动态目标 QPS
        </label>
        <input
          v-model.number="tuneQps"
          type="number"
          min="1"
          :disabled="!isRunning"
          class="input font-mono w-32 !py-1.5"
        />
      </div>
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          在线动态并发上限
        </label>
        <input
          v-model.number="tuneConcurrency"
          type="number"
          min="1"
          :disabled="!isRunning"
          class="input font-mono w-32 !py-1.5"
        />
      </div>
      <HoppButtonPrimary
        :disabled="!isRunning"
        type="button"
        label="热更新流量参数"
        @click="applyTrafficPatch"
      />
      <span class="text-tiny text-secondaryLight pb-1.5">
        运行期间可随时无损调高或降低 QPS 与并发，即时下发至全部工作进程
      </span>
    </form>

    <!-- ECharts Live Throughput & Success/Failure Series -->
    <div class="p-3 rounded border border-dividerLight bg-primaryLight/20">
      <div class="text-tiny font-semibold text-secondaryDark mb-2">
        每秒吞吐量与成功/失败趋势 (Real-time Timeseries)
      </div>
      <div ref="chartEl" class="w-full h-64" />
    </div>

    <!-- Bottom Detailed Tabs: Samples Explorer / SLA Verdict / Windows / Timeline / Snapshot -->
    <div class="border border-dividerLight rounded overflow-hidden bg-primary">
      <HoppSmartTabs v-model="activeDetailTab" render-inactive-tabs>
        <!-- 1. Samples Explorer -->
        <HoppSmartTab
          id="samples"
          label="请求样本明细 (Samples)"
          :info="samples.length ? String(samples.length) : null"
        >
          <div class="p-4 space-y-3">
            <div class="flex flex-wrap items-end gap-3 pb-3 border-b border-dividerLight">
              <div>
                <label class="block text-tiny text-secondaryLight mb-1">结果状态</label>
                <select v-model="sampleFilterSuccess" class="select !py-1 text-tiny w-36">
                  <option value="all">全部样本</option>
                  <option value="false">仅失败请求</option>
                  <option value="true">仅成功请求</option>
                </select>
              </div>
              <div>
                <label class="block text-tiny text-secondaryLight mb-1">错误分类 (error_class)</label>
                <input
                  v-model="sampleFilterErrClass"
                  type="text"
                  placeholder="如 http_5xx / timeout"
                  class="input font-mono !py-1 text-tiny w-44"
                />
              </div>
              <div>
                <label class="block text-tiny text-secondaryLight mb-1">最低延迟 (ms)</label>
                <input
                  v-model="sampleFilterMinMs"
                  type="number"
                  min="0"
                  placeholder="0"
                  class="input font-mono !py-1 text-tiny w-28"
                />
              </div>
              <HoppButtonSecondary label="检索样本" outline @click="loadSamples(true)" />
            </div>

            <div v-if="samples.length === 0" class="py-8 text-center text-secondaryLight text-tiny">
              暂无匹配的请求明细样本
            </div>

            <div v-else class="overflow-x-auto">
              <table class="w-full text-left border-collapse text-tiny font-mono">
                <thead>
                  <tr class="border-b border-dividerLight text-secondaryLight">
                    <th class="py-2 pr-3">Worker#Seq</th>
                    <th class="py-2 pr-3">接口 / Product</th>
                    <th class="py-2 pr-3">状态</th>
                    <th class="py-2 pr-3">延迟</th>
                    <th class="py-2 pr-3">字节</th>
                    <th class="py-2">错误 / 诊断信息</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-dividerLight">
                  <tr v-for="s in samples" :key="`${s.worker}-${s.seq}`" class="hover:bg-primaryLight/40">
                    <td class="py-1.5 pr-3 text-secondaryLight">w{{ s.worker }}#{{ s.seq }}</td>
                    <td class="py-1.5 pr-3 text-secondaryDark">
                      {{ s.product_name || s.product_id }}
                    </td>
                    <td class="py-1.5 pr-3">
                      <span :class="s.success ? 'text-emerald-400' : 'text-rose-400'">
                        {{ s.http_status ?? (s.success ? "OK" : "ERR") }}
                      </span>
                    </td>
                    <td class="py-1.5 pr-3 text-secondaryDark">{{ formatLatencyUs(s.latency_us) }}</td>
                    <td class="py-1.5 pr-3 text-secondaryLight">{{ formatBytes(s.response_bytes) }}</td>
                    <td class="py-1.5 text-rose-400 truncate max-w-md">
                      {{ s.error_class ? `[${s.error_class}] ` : "" }}{{ s.error ?? "" }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div v-if="sampleNextCursor" class="flex justify-center pt-2">
              <HoppButtonSecondary label="加载更多样本" outline @click="loadSamples(false)" />
            </div>
          </div>
        </HoppSmartTab>

        <!-- 2. SLA Verdict -->
        <HoppSmartTab id="verdict" label="SLA 判定 (Verdict)">
          <div class="p-4 space-y-3 text-tiny">
            <div v-if="!run?.verdict || run.verdict.result === 'none'" class="text-secondaryLight py-4">
              当前压测未配置 SLA 阈值判定规则 (AssertSpec)。
            </div>
            <template v-else>
              <div class="font-bold text-secondaryDark">
                综合结论：
                <span :class="verdictClass(run.verdict.result)" class="px-2 py-0.5 rounded border font-mono">
                  {{ run.verdict.result.toUpperCase() }}
                </span>
              </div>
              <div v-if="run.verdict.failed?.length" class="space-y-1">
                <div
                  v-for="fc in run.verdict.failed"
                  :key="fc.check"
                  class="px-3 py-2 rounded border border-rose-500/30 bg-rose-500/10 font-mono text-rose-400"
                >
                  未达标项 {{ fc.check }}：目标阈值 {{ fc.wanted }}，实测值 {{ fc.actual ?? "-" }}
                </div>
              </div>
              <pre
                v-if="run.verdict.actual"
                class="p-3 rounded border border-dividerLight bg-primaryLight font-mono text-secondaryDark"
              >{{ JSON.stringify(run.verdict.actual, null, 2) }}</pre>
            </template>
          </div>
        </HoppSmartTab>

        <!-- 3. Loop Windows -->
        <HoppSmartTab
          id="windows"
          label="循环窗口 (Windows)"
          :info="windows.length ? String(windows.length) : null"
        >
          <div class="p-4 overflow-x-auto">
            <table class="w-full text-left border-collapse text-tiny font-mono">
              <thead>
                <tr class="border-b border-dividerLight text-secondaryLight">
                  <th class="py-2 pr-3">轮次 #</th>
                  <th class="py-2 pr-3">样本数</th>
                  <th class="py-2 pr-3">成功</th>
                  <th class="py-2 pr-3">失败</th>
                  <th class="py-2 pr-3">p50</th>
                  <th class="py-2 pr-3">p95</th>
                  <th class="py-2">p99</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-dividerLight">
                <tr v-for="w in windows" :key="w.window_index">
                  <td class="py-1.5 pr-3">#{{ w.window_index }}</td>
                  <td class="py-1.5 pr-3">{{ w.samples }}</td>
                  <td class="py-1.5 pr-3 text-emerald-400">{{ w.success }}</td>
                  <td class="py-1.5 pr-3 text-rose-400">{{ w.failure }}</td>
                  <td class="py-1.5 pr-3">{{ formatLatencyUs(w.p50_us) }}</td>
                  <td class="py-1.5 pr-3">{{ formatLatencyUs(w.p95_us) }}</td>
                  <td class="py-1.5">{{ formatLatencyUs(w.p99_us) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </HoppSmartTab>

        <!-- 4. Lifecycle Timeline -->
        <HoppSmartTab
          id="timeline"
          label="事件时间线 (Timeline)"
          :info="timeline.length ? String(timeline.length) : null"
        >
          <div class="p-4 divide-y divide-dividerLight text-tiny font-mono">
            <div v-for="ev in timeline" :key="ev.id" class="py-2 flex items-center justify-between">
              <div>
                <span class="text-accent font-bold mr-3">{{ ev.event }}</span>
                <span v-if="ev.payload" class="text-secondary">
                  {{ JSON.stringify(ev.payload) }}
                </span>
              </div>
              <span class="text-secondaryLight">{{ formatTimeMs(ev.ts) }}</span>
            </div>
          </div>
        </HoppSmartTab>

        <!-- 5. Redacted Snapshot -->
        <HoppSmartTab id="snapshot" label="执行快照 (Snapshot)">
          <div class="p-4">
            <CodeMirrorEditor
              :model-value="JSON.stringify(run?.snapshot ?? {}, null, 2)"
              language="json"
              readonly
              min-height="220px"
              max-height="460px"
            />
          </div>
        </HoppSmartTab>
      </HoppSmartTabs>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import * as echarts from "echarts";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartTab,
  HoppSmartTabs,
} from "@hoppscotch/ui";
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
import {
  formatBytes,
  formatLatencyUs,
  formatTimeMs,
} from "../helpers/format";
import { toast } from "../modules/toast";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id));

const run = ref<RunView | null>(null);
const stats = ref<RunStats | null>(null);
const windows = ref<WindowRow[]>([]);
const timeline = ref<TimelineEvent[]>([]);
const samples = ref<SampleRow[]>([]);
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
const activeDetailTab = ref("samples");

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

const metricCards = computed(() => {
  const s = stats.value;
  return [
    { label: "样本总数", value: s?.samples ?? 0 },
    {
      label: "成功请求",
      value: s?.success ?? 0,
      colorClass: "text-emerald-400",
    },
    {
      label: "失败请求",
      value: s?.failure ?? 0,
      colorClass: (s?.failure ?? 0) > 0 ? "text-rose-400" : "text-secondaryDark",
    },
    { label: "p50 延迟", value: formatLatencyUs(s?.p50_us) },
    { label: "p95 延迟", value: formatLatencyUs(s?.p95_us) },
    {
      label: "p99 延迟",
      value: formatLatencyUs(s?.p99_us),
      colorClass: "text-accent",
    },
    { label: "在途并发", value: s?.in_flight ?? 0 },
    { label: "累计字节", value: formatBytes(s?.bytes) },
  ];
});

function verdictClass(result: string): string {
  if (result === "pass") {
    return "border-emerald-500/40 bg-emerald-500/15 text-emerald-400";
  }
  if (result === "fail") {
    return "border-rose-500/40 bg-rose-500/15 text-rose-400";
  }
  return "border-amber-500/40 bg-amber-500/15 text-amber-400";
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
  if (effectiveSeries.length === 0 && statsData.samples > 0) {
    try {
      const report = await api.getRunReport(id.value);
      effectiveSeries = report.timeseries ?? [];
    } catch {
      // Keep empty series if report is still being finalized
    }
  }

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
  const labels = points.map((p) => {
    const sec = p.unix_sec ?? p.started_at;
    return sec
      ? new Date(sec * 1000).toLocaleTimeString("zh-CN", { hour12: false })
      : "";
  });
  chart.setOption({
    backgroundColor: "transparent",
    tooltip: { trigger: "axis" },
    grid: { left: 42, right: 20, top: 30, bottom: 24 },
    legend: {
      data: ["总样本/s", "成功/s", "失败/s"],
      textStyle: { color: "#9ca3af", fontSize: 11 },
    },
    xAxis: {
      type: "category",
      data: labels,
      axisLabel: { color: "#9ca3af", fontSize: 10 },
    },
    yAxis: {
      type: "value",
      splitLine: { lineStyle: { color: "rgba(148,163,184,0.12)" } },
      axisLabel: { color: "#9ca3af", fontSize: 10 },
    },
    series: [
      {
        name: "总样本/s",
        type: "line",
        smooth: true,
        data: points.map((p) => p.samples || 0),
        itemStyle: { color: "#38bdf8" },
      },
      {
        name: "成功/s",
        type: "line",
        smooth: true,
        data: points.map((p) => p.success || 0),
        itemStyle: { color: "#10b981" },
      },
      {
        name: "失败/s",
        type: "line",
        smooth: true,
        data: points.map((p) => p.failure || 0),
        itemStyle: { color: "#f43f5e" },
      },
    ],
  });
}

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
