<template>
  <div class="space-y-5">
    <!-- 6-Chart Core Matrix -->
    <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">吞吐与事件趋势</h2>
          <span v-if="hasWarmup" class="font-mono text-[11px] text-amber-600 dark:text-amber-400">
            阴影区为预热期
          </span>
        </div>
        <div class="relative h-64 w-full">
          <div ref="throughputEl" class="h-full w-full" />
          <div
            v-if="series.length === 0"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无秒级吞吐数据
          </div>
        </div>
      </UiCard>

      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">响应延迟分位趋势</h2>
          <span class="font-mono text-[11px] text-muted-foreground">ms</span>
        </div>
        <div class="relative h-64 w-full">
          <div ref="latencyEl" class="h-full w-full" />
          <div
            v-if="!hasLatencySeries"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无秒级延迟数据
          </div>
        </div>
      </UiCard>

      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">吞吐与延迟拐点关联</h2>
          <div class="inline-flex rounded-md border border-border bg-muted p-0.5 text-[11px]">
            <button
              type="button"
              class="cursor-pointer rounded-md px-2 py-0.5 transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
              :class="corrMode === 'dual' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'"
              @click="corrMode = 'dual'"
            >
              时序双轴
            </button>
            <button
              type="button"
              class="cursor-pointer rounded-md px-2 py-0.5 transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
              :class="corrMode === 'scatter' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'"
              @click="corrMode = 'scatter'"
            >
              拐点散点
            </button>
          </div>
        </div>
        <div class="relative h-64 w-full">
          <div ref="corrEl" class="h-full w-full" />
          <div
            v-if="!hasLatencySeries"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无关联数据
          </div>
        </div>
      </UiCard>

      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">延迟分布与 Apdex</h2>
          <div v-if="apdex" class="flex flex-wrap gap-1.5 font-mono text-[11px] tabular-nums">
            <span class="text-emerald-600 dark:text-emerald-400">满意 {{ apdex.satisfied }}</span>
            <span class="text-amber-600 dark:text-amber-400">容忍 {{ apdex.tolerated }}</span>
            <span class="text-rose-600 dark:text-rose-400">缓慢 {{ apdex.frustrated }}</span>
            <span v-if="apdex.failed > 0" class="text-rose-600 dark:text-rose-400">失败 {{ apdex.failed }}</span>
          </div>
        </div>
        <div class="relative h-64 w-full">
          <div ref="histEl" class="h-full w-full" />
          <div
            v-if="histogram.length === 0"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无延迟分布样本
          </div>
        </div>
      </UiCard>

      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">错误率与状态码</h2>
          <div v-if="statusEntries.length" class="flex flex-wrap gap-1 font-mono text-[10px] tabular-nums">
            <span
              v-for="[code, count] in statusEntries.slice(0, 5)"
              :key="code"
              class="rounded border border-border bg-muted/50 px-1.5 py-0.5 text-foreground"
            >
              {{ code === "none" ? "无响应" : code }}×{{ count }}
            </span>
          </div>
        </div>
        <div class="relative h-64 w-full">
          <div ref="errEl" class="h-full w-full" />
          <div
            v-if="series.length === 0"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无错误趋势数据
          </div>
        </div>
      </UiCard>

      <UiCard class="space-y-2 p-4 sm:p-5">
        <div class="flex items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-foreground">网络带宽趋势</h2>
          <span class="font-mono text-[11px] text-muted-foreground">KB/s</span>
        </div>
        <div class="relative h-64 w-full">
          <div ref="bwEl" class="h-full w-full" />
          <div
            v-if="series.length === 0"
            class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            暂无带宽采样数据
          </div>
        </div>
      </UiCard>
    </div>

    <!-- Adaptive Protocol Breakdown: HTTP Generic -->
    <UiCard v-if="httpBreakdown" class="space-y-4 p-4 sm:p-5">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">HTTP 通用协议拆解</h2>
          <UiBadge variant="outline" class="font-mono text-[10px]">
            {{ httpBreakdown.samples }} 样本
          </UiBadge>
        </div>
        <div class="flex flex-wrap gap-3 font-mono text-xs text-muted-foreground tabular-nums">
          <span>
            方法 {{ Object.entries(httpBreakdown.methods).map(([m, n]) => `${m}×${n}`).join(" ") || "-" }}
          </span>
          <span>均响应体 {{ formatBytes(httpBreakdown.response_sizes.avg_bytes) }}</span>
          <span v-if="httpBreakdown.request_sizes.avg_bytes != null">
            均请求体 {{ formatBytes(httpBreakdown.request_sizes.avg_bytes) }}
          </span>
        </div>
      </div>

      <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <div>
          <div class="mb-2 text-xs font-medium text-muted-foreground">
            阶段耗时拆解（调度排队 / 首字节 TTFB / 响应体传输）
          </div>
          <div ref="httpPhasesEl" class="h-56 w-full" />
        </div>
        <div>
          <div class="mb-2 text-xs font-medium text-muted-foreground">
            响应体体积分布
          </div>
          <div ref="httpSizesEl" class="h-56 w-full" />
        </div>
      </div>

      <div class="overflow-x-auto">
        <table class="w-full border-collapse text-left font-mono text-xs tabular-nums">
          <thead>
            <tr class="border-b border-border text-muted-foreground">
              <th class="py-2 pr-4 font-medium">阶段</th>
              <th class="py-2 pr-4 font-medium">样本</th>
              <th class="py-2 pr-4 font-medium">Avg</th>
              <th class="py-2 pr-4 font-medium">P50</th>
              <th class="py-2 pr-4 font-medium">P95</th>
              <th class="py-2 pr-4 font-medium">P99</th>
              <th class="py-2 pr-4 font-medium">Max</th>
              <th class="py-2 font-medium">耗时占比</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            <tr v-for="row in httpPhaseRows" :key="row.label">
              <td class="py-2 pr-4 font-sans font-medium text-foreground">{{ row.label }}</td>
              <td class="py-2 pr-4">{{ row.stats.samples }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.avg_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p50_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p95_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p99_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.max_us) }}</td>
              <td class="py-2 text-muted-foreground">
                {{ row.stats.share_pct != null ? `${row.stats.share_pct}%` : "-" }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </UiCard>

    <!-- Adaptive Protocol Breakdown: Mazbri Gateway -->
    <UiCard v-if="mazbriBreakdown" class="space-y-4 p-4 sm:p-5">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">Mazbri 网关协议拆解</h2>
          <UiBadge variant="outline" class="font-mono text-[10px]">
            {{ mazbriBreakdown.samples }} 样本
          </UiBadge>
        </div>
        <div class="flex flex-wrap gap-3 font-mono text-xs text-muted-foreground tabular-nums">
          <span>加密 {{ mazbriBreakdown.encryption_modes.join(", ") || "-" }}</span>
          <span>套件 {{ mazbriBreakdown.suites.join(", ") || "-" }}</span>
          <span v-if="mazbriBreakdown.crypto_overhead_pct != null" class="text-violet-600 dark:text-violet-400">
            本地密码开销 {{ mazbriBreakdown.crypto_overhead_pct }}%
          </span>
          <span v-if="mazbriBreakdown.wire_pct != null">
            网关往返 {{ mazbriBreakdown.wire_pct }}%
          </span>
          <span>
            Trace 一致 {{ mazbriBreakdown.trace_echo.match }} / 不一致 {{ mazbriBreakdown.trace_echo.mismatch }}
          </span>
        </div>
      </div>

      <div ref="mazbriPhasesEl" class="h-60 w-full" />

      <div class="overflow-x-auto">
        <table class="w-full border-collapse text-left font-mono text-xs tabular-nums">
          <thead>
            <tr class="border-b border-border text-muted-foreground">
              <th class="py-2 pr-4 font-medium">阶段</th>
              <th class="py-2 pr-4 font-medium">样本</th>
              <th class="py-2 pr-4 font-medium">Avg</th>
              <th class="py-2 pr-4 font-medium">P50</th>
              <th class="py-2 pr-4 font-medium">P95</th>
              <th class="py-2 pr-4 font-medium">P99</th>
              <th class="py-2 pr-4 font-medium">Max</th>
              <th class="py-2 font-medium">占比</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            <tr v-for="row in mazbriPhaseRows" :key="row.label">
              <td class="py-2 pr-4 font-sans font-medium text-foreground">{{ row.label }}</td>
              <td class="py-2 pr-4">{{ row.stats.samples }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.avg_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p50_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p95_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.p99_us) }}</td>
              <td class="py-2 pr-4">{{ formatLatencyUs(row.stats.max_us) }}</td>
              <td class="py-2 text-muted-foreground">
                {{ row.stats.share_pct != null ? `${row.stats.share_pct}%` : "-" }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </UiCard>

    <!-- Multi-Endpoint & Multi-Window Adaptive Charts -->
    <div
      v-if="hasMultiEndpoints || hasMultiWindows"
      class="grid grid-cols-1 gap-5"
      :class="hasMultiEndpoints && hasMultiWindows ? 'lg:grid-cols-2' : ''"
    >
      <UiCard v-if="hasMultiEndpoints" class="space-y-2 p-4 sm:p-5">
        <h2 class="text-sm font-semibold text-foreground">多接口延迟对比 (ms)</h2>
        <div ref="endpointsEl" class="h-64 w-full" />
      </UiCard>
      <UiCard v-if="hasMultiWindows" class="space-y-2 p-4 sm:p-5">
        <h2 class="text-sm font-semibold text-foreground">循环轮次演化</h2>
        <div ref="windowsEl" class="h-64 w-full" />
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { init as initChart, use as useECharts, type EChartsType } from "echarts/core";
import { BarChart, LineChart, ScatterChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type {
  ApdexStats,
  ReportBreakdown,
  TimePoint,
  TimelineEvent,
  WindowRow,
} from "../../api/types";
import { formatBytes, formatLatencyUs } from "../../helpers/format";
import { effectiveTheme } from "../../modules/theme";
import UiBadge from "../ui/UiBadge.vue";
import UiCard from "../ui/UiCard.vue";

useECharts([
  BarChart,
  LineChart,
  ScatterChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  CanvasRenderer,
]);

const props = defineProps<{
  series: TimePoint[];
  breakdown: ReportBreakdown | null;
  timeline: TimelineEvent[];
  windows: WindowRow[];
  apdex?: ApdexStats | null;
}>();

const corrMode = ref<"dual" | "scatter">("dual");

const throughputEl = ref<HTMLDivElement | null>(null);
const latencyEl = ref<HTMLDivElement | null>(null);
const corrEl = ref<HTMLDivElement | null>(null);
const histEl = ref<HTMLDivElement | null>(null);
const errEl = ref<HTMLDivElement | null>(null);
const bwEl = ref<HTMLDivElement | null>(null);
const httpPhasesEl = ref<HTMLDivElement | null>(null);
const httpSizesEl = ref<HTMLDivElement | null>(null);
const mazbriPhasesEl = ref<HTMLDivElement | null>(null);
const endpointsEl = ref<HTMLDivElement | null>(null);
const windowsEl = ref<HTMLDivElement | null>(null);

const chartInstances = new Map<string, EChartsType>();

const hasWarmup = computed(() => props.series.some((p) => p.warmup));
const hasLatencySeries = computed(() =>
  props.series.some((p) => p.p50_us != null || p.avg_latency_us != null),
);
const histogram = computed(() => props.breakdown?.latency_histogram ?? []);
const statusEntries = computed(() =>
  Object.entries(props.breakdown?.status_codes ?? {}),
);
const httpBreakdown = computed(
  () => props.breakdown?.protocol_breakdown?.http ?? null,
);
const mazbriBreakdown = computed(
  () => props.breakdown?.protocol_breakdown?.mazbri ?? null,
);
const hasMultiEndpoints = computed(
  () => (props.breakdown?.products.length ?? 0) > 1,
);
const hasMultiWindows = computed(() => props.windows.length > 1);

const httpPhaseRows = computed(() => {
  const hp = httpBreakdown.value?.phases;
  if (!hp) return [];
  return [
    { label: "调度排队", stats: hp.queue },
    { label: "首字节 TTFB", stats: hp.ttfb },
    { label: "响应体传输", stats: hp.transfer },
    { label: "总耗时", stats: hp.total },
  ];
});

const mazbriPhaseRows = computed(() => {
  const mp = mazbriBreakdown.value?.phases;
  if (!mp) return [];
  return [
    { label: "令牌等待", stats: mp.token_wait },
    { label: "本地加密签名", stats: mp.crypto_seal },
    { label: "网关 TTFB", stats: mp.gateway_ttfb },
    { label: "密文传输", stats: mp.transfer },
    { label: "本地验签解密", stats: mp.crypto_unseal },
    { label: "端到端总计", stats: mp.end_to_end },
  ];
});

function getOrInit(key: string, el: HTMLDivElement | null): EChartsType | null {
  if (!el) {
    const old = chartInstances.get(key);
    if (old) {
      old.dispose();
      chartInstances.delete(key);
    }
    return null;
  }
  const existing = chartInstances.get(key);
  if (existing && existing.getDom() === el) return existing;
  existing?.dispose();
  const c = initChart(el);
  chartInstances.set(key, c);
  return c;
}

function toMs(v: number | null | undefined): number | null {
  return v == null ? null : Number((v / 1000).toFixed(2));
}

function renderAll(): void {
  const isDark = effectiveTheme.value === "black";
  const axisColor = isDark ? "#a1a1aa" : "#71717a";
  const gridColor = isDark ? "rgba(255,255,255,0.08)" : "rgba(9,9,11,0.07)";
  const primary = isDark ? "#fafafa" : "#18181b";
  const ok = isDark ? "#34d399" : "#059669";
  const warn = isDark ? "#fbbf24" : "#d97706";
  const err = isDark ? "#fb7185" : "#e11d48";
  const blue = isDark ? "#60a5fa" : "#2563eb";
  const violet = isDark ? "#a78bfa" : "#7c3aed";

  const tooltip = {
    trigger: "axis" as const,
    backgroundColor: isDark ? "#111114" : "#ffffff",
    borderColor: isDark ? "#27272a" : "#e4e4e7",
    textStyle: { color: isDark ? "#fafafa" : "#09090b", fontSize: 12 },
  };
  const labels = props.series.map((p) => {
    const raw = p.unix_sec ?? p.started_at;
    if (!raw) return "";
    const ms = raw > 1e11 ? raw : raw * 1000;
    return new Date(ms).toLocaleTimeString("zh-CN", { hour12: false });
  });
  const showDot = props.series.length <= 2;

  // Warmup markArea & timeline markLines
  const warmupAreas: { xAxis: string; name?: string }[][] = [];
  let wStart: number | null = null;
  props.series.forEach((p, idx) => {
    if (p.warmup && wStart === null) wStart = idx;
    if ((!p.warmup || idx === props.series.length - 1) && wStart !== null) {
      const endIdx = p.warmup ? idx : idx - 1;
      warmupAreas.push([
        { xAxis: labels[wStart]!, name: "预热" },
        { xAxis: labels[endIdx]! },
      ]);
      wStart = null;
    }
  });
  const eventLines: { xAxis: string; name: string; label: { formatter: string; color: string } }[] = [];
  for (const ev of props.timeline) {
    if (!["traffic", "resume", "interrupted", "stop"].includes(ev.event)) continue;
    const sec = Math.floor(ev.ts / 1000);
    const idx = props.series.findIndex((p) => p.unix_sec === sec);
    if (idx >= 0) {
      const lbl =
        ev.event === "traffic"
          ? "调参"
          : ev.event === "resume"
            ? "续跑"
            : ev.event === "interrupted"
              ? "中断"
              : "停止";
      eventLines.push({
        xAxis: labels[idx]!,
        name: lbl,
        label: { formatter: lbl, color: warn },
      });
    }
  }

  // 1. Throughput
  const cTp = getOrInit("throughput", throughputEl.value);
  if (cTp) {
    cTp.setOption(
      {
        backgroundColor: "transparent",
        tooltip,
        grid: { left: 46, right: 16, top: 34, bottom: 26 },
        legend: { data: ["总样本/s", "成功/s", "失败/s"], textStyle: { color: axisColor, fontSize: 11 } },
        xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        yAxis: { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        series: [
          {
            name: "总样本/s",
            type: "line",
            showSymbol: showDot,
            lineStyle: { width: 2, type: "dashed" },
            itemStyle: { color: primary },
            data: props.series.map((p) => p.samples || 0),
            markArea: warmupAreas.length ? { itemStyle: { color: "rgba(217,119,6,0.10)" }, data: warmupAreas } : undefined,
            markLine: eventLines.length ? { symbol: "none", lineStyle: { color: warn, type: "dotted" }, data: eventLines } : undefined,
          },
          { name: "成功/s", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: ok }, data: props.series.map((p) => p.success || 0) },
          { name: "失败/s", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: err }, data: props.series.map((p) => p.failure || 0) },
        ],
      },
      true,
    );
  }

  // 2. Latency Percentiles Over Time
  const cLat = getOrInit("latency", latencyEl.value);
  if (cLat) {
    cLat.setOption(
      {
        backgroundColor: "transparent",
        tooltip,
        grid: { left: 48, right: 16, top: 34, bottom: 26 },
        legend: { data: ["Avg", "P50", "P90", "P95", "P99"], textStyle: { color: axisColor, fontSize: 11 } },
        xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        yAxis: { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        series: [
          { name: "Avg", type: "line", showSymbol: showDot, lineStyle: { width: 1.5, type: "dashed" }, itemStyle: { color: axisColor }, data: props.series.map((p) => toMs(p.avg_latency_us)) },
          { name: "P50", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: ok }, data: props.series.map((p) => toMs(p.p50_us)) },
          { name: "P90", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: blue }, data: props.series.map((p) => toMs(p.p90_us)) },
          { name: "P95", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: warn }, data: props.series.map((p) => toMs(p.p95_us)) },
          { name: "P99", type: "line", showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: err }, data: props.series.map((p) => toMs(p.p99_us)) },
        ],
      },
      true,
    );
  }

  // 3. Load vs Latency Correlation
  const cCorr = getOrInit("corr", corrEl.value);
  if (cCorr) {
    if (corrMode.value === "dual") {
      cCorr.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 46, right: 50, top: 34, bottom: 26 },
          legend: { data: ["QPS", "峰值并发", "P95 (ms)", "P99 (ms)"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: [
            { type: "value", name: "req/s", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
            { type: "value", name: "ms", splitLine: { show: false }, axisLabel: { color: axisColor, fontSize: 11 } },
          ],
          series: [
            { name: "QPS", type: "bar", yAxisIndex: 0, itemStyle: { color: isDark ? "rgba(96,165,250,0.28)" : "rgba(37,99,235,0.2)" }, data: props.series.map((p) => p.samples || 0) },
            { name: "峰值并发", type: "line", yAxisIndex: 0, showSymbol: showDot, lineStyle: { width: 1.5, type: "dashed" }, itemStyle: { color: violet }, data: props.series.map((p) => p.in_flight_max ?? 0) },
            { name: "P95 (ms)", type: "line", yAxisIndex: 1, showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: warn }, data: props.series.map((p) => toMs(p.p95_us)) },
            { name: "P99 (ms)", type: "line", yAxisIndex: 1, showSymbol: showDot, lineStyle: { width: 2 }, itemStyle: { color: err }, data: props.series.map((p) => toMs(p.p99_us)) },
          ],
        },
        true,
      );
    } else {
      const pts95 = props.series
        .filter((p) => !p.warmup && p.p95_us != null)
        .map((p) => [p.samples, toMs(p.p95_us)]);
      const pts99 = props.series
        .filter((p) => !p.warmup && p.p99_us != null)
        .map((p) => [p.samples, toMs(p.p99_us)]);
      cCorr.setOption(
        {
          backgroundColor: "transparent",
          tooltip: {
            trigger: "item",
            backgroundColor: isDark ? "#111114" : "#ffffff",
            borderColor: isDark ? "#27272a" : "#e4e4e7",
            textStyle: { color: isDark ? "#fafafa" : "#09090b", fontSize: 12 },
          },
          grid: { left: 50, right: 20, top: 34, bottom: 32 },
          legend: { data: ["P95 (ms)", "P99 (ms)"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "value", name: "QPS", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          series: [
            { name: "P95 (ms)", type: "scatter", symbolSize: 7, itemStyle: { color: warn }, data: pts95 },
            { name: "P99 (ms)", type: "scatter", symbolSize: 7, itemStyle: { color: err }, data: pts99 },
          ],
        },
        true,
      );
    }
  }

  // 4. Latency Distribution Histogram + CDF
  const cHist = getOrInit("hist", histEl.value);
  if (cHist) {
    cHist.setOption(
      {
        backgroundColor: "transparent",
        tooltip,
        grid: { left: 48, right: 44, top: 34, bottom: 44 },
        legend: { data: ["请求数", "累计 %"], textStyle: { color: axisColor, fontSize: 11 } },
        xAxis: {
          type: "category",
          data: histogram.value.map((b) => b.label),
          axisLine: { lineStyle: { color: gridColor } },
          axisLabel: { color: axisColor, fontSize: 10, rotate: 28 },
        },
        yAxis: [
          { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          { type: "value", max: 100, name: "%", splitLine: { show: false }, axisLabel: { color: axisColor, fontSize: 11 } },
        ],
        series: [
          { name: "请求数", type: "bar", yAxisIndex: 0, itemStyle: { color: blue, borderRadius: [3, 3, 0, 0] }, data: histogram.value.map((b) => b.count) },
          { name: "累计 %", type: "line", yAxisIndex: 1, smooth: true, symbolSize: 5, itemStyle: { color: ok }, data: histogram.value.map((b) => b.cumulative_pct) },
        ],
      },
      true,
    );
  }

  // 5. Errors Over Time
  const cErr = getOrInit("err", errEl.value);
  if (cErr) {
    cErr.setOption(
      {
        backgroundColor: "transparent",
        tooltip,
        grid: { left: 46, right: 44, top: 34, bottom: 26 },
        legend: { data: ["错误率 %", "失败/s"], textStyle: { color: axisColor, fontSize: 11 } },
        xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        yAxis: [
          { type: "value", name: "%", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          { type: "value", name: "个/s", splitLine: { show: false }, axisLabel: { color: axisColor, fontSize: 11 } },
        ],
        series: [
          { name: "错误率 %", type: "line", yAxisIndex: 0, showSymbol: showDot, itemStyle: { color: err }, areaStyle: { color: "rgba(225,29,72,0.12)" }, data: props.series.map((p) => Number(((p.error_rate ?? 0) * 100).toFixed(2))) },
          { name: "失败/s", type: "bar", yAxisIndex: 1, itemStyle: { color: warn }, data: props.series.map((p) => p.failure || 0) },
        ],
      },
      true,
    );
  }

  // 6. Bandwidth
  const cBw = getOrInit("bw", bwEl.value);
  if (cBw) {
    const toKB = (v: number | undefined) => Number(((v ?? 0) / 1024).toFixed(2));
    cBw.setOption(
      {
        backgroundColor: "transparent",
        tooltip,
        grid: { left: 50, right: 16, top: 34, bottom: 26 },
        legend: { data: ["下行响应 KB/s", "上行请求 KB/s"], textStyle: { color: axisColor, fontSize: 11 } },
        xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        yAxis: { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
        series: [
          { name: "下行响应 KB/s", type: "line", showSymbol: showDot, itemStyle: { color: blue }, areaStyle: { color: "rgba(37,99,235,0.12)" }, data: props.series.map((p) => toKB(p.bytes)) },
          { name: "上行请求 KB/s", type: "line", showSymbol: showDot, itemStyle: { color: violet }, data: props.series.map((p) => toKB(p.request_bytes)) },
        ],
      },
      true,
    );
  }

  // 7a. HTTP Generic Breakdown
  if (httpBreakdown.value) {
    const hp = httpBreakdown.value.phases;
    const cHp = getOrInit("httpPhases", httpPhasesEl.value);
    if (cHp) {
      cHp.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 48, right: 16, top: 30, bottom: 24 },
          legend: { data: ["Avg", "P95", "P99"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "category", data: ["调度排队", "首字节 TTFB", "响应体传输"], axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          series: [
            { name: "Avg", type: "bar", itemStyle: { color: blue }, data: [hp.queue, hp.ttfb, hp.transfer].map((x) => toMs(x.avg_us) ?? 0) },
            { name: "P95", type: "bar", itemStyle: { color: warn }, data: [hp.queue, hp.ttfb, hp.transfer].map((x) => toMs(x.p95_us) ?? 0) },
            { name: "P99", type: "bar", itemStyle: { color: err }, data: [hp.queue, hp.ttfb, hp.transfer].map((x) => toMs(x.p99_us) ?? 0) },
          ],
        },
        true,
      );
    }
    const cHs = getOrInit("httpSizes", httpSizesEl.value);
    if (cHs) {
      const buckets = httpBreakdown.value.response_sizes.buckets;
      cHs.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 48, right: 16, top: 20, bottom: 24 },
          xAxis: { type: "category", data: buckets.map((b) => b.label), axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          series: [{ name: "响应数", type: "bar", itemStyle: { color: ok, borderRadius: [3, 3, 0, 0] }, data: buckets.map((b) => b.count) }],
        },
        true,
      );
    }
  }

  // 7b. Mazbri Breakdown
  if (mazbriBreakdown.value) {
    const mp = mazbriBreakdown.value.phases;
    const cMp = getOrInit("mazbriPhases", mazbriPhasesEl.value);
    if (cMp) {
      const items = [
        ["令牌等待", mp.token_wait],
        ["本地加密签名", mp.crypto_seal],
        ["网关 TTFB", mp.gateway_ttfb],
        ["密文传输", mp.transfer],
        ["本地验签解密", mp.crypto_unseal],
      ] as const;
      cMp.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 48, right: 16, top: 32, bottom: 26 },
          legend: { data: ["Avg", "P95", "P99"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "category", data: items.map((x) => x[0]), axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          series: [
            { name: "Avg", type: "bar", itemStyle: { color: violet }, data: items.map((x) => toMs(x[1].avg_us) ?? 0) },
            { name: "P95", type: "bar", itemStyle: { color: warn }, data: items.map((x) => toMs(x[1].p95_us) ?? 0) },
            { name: "P99", type: "bar", itemStyle: { color: err }, data: items.map((x) => toMs(x[1].p99_us) ?? 0) },
          ],
        },
        true,
      );
    }
  }

  // 7c. Multi-Endpoint Comparison
  if (hasMultiEndpoints.value && props.breakdown) {
    const cEp = getOrInit("endpoints", endpointsEl.value);
    if (cEp) {
      const prods = props.breakdown.products;
      cEp.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 48, right: 16, top: 32, bottom: 28 },
          legend: { data: ["P50", "P95", "P99"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "category", data: prods.map((p) => p.product_id ?? "-"), axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          series: [
            { name: "P50", type: "bar", itemStyle: { color: ok }, data: prods.map((p) => toMs(p.p50_us) ?? 0) },
            { name: "P95", type: "bar", itemStyle: { color: warn }, data: prods.map((p) => toMs(p.p95_us) ?? 0) },
            { name: "P99", type: "bar", itemStyle: { color: err }, data: prods.map((p) => toMs(p.p99_us) ?? 0) },
          ],
        },
        true,
      );
    }
  }

  // 7d. Loop Windows Evolution
  if (hasMultiWindows.value) {
    const cWin = getOrInit("windows", windowsEl.value);
    if (cWin) {
      cWin.setOption(
        {
          backgroundColor: "transparent",
          tooltip,
          grid: { left: 48, right: 50, top: 32, bottom: 26 },
          legend: { data: ["样本数", "P95 (ms)", "P99 (ms)"], textStyle: { color: axisColor, fontSize: 11 } },
          xAxis: { type: "category", data: props.windows.map((w) => `#${w.window_index}`), axisLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
          yAxis: [
            { type: "value", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor, fontSize: 11 } },
            { type: "value", name: "ms", splitLine: { show: false }, axisLabel: { color: axisColor, fontSize: 11 } },
          ],
          series: [
            { name: "样本数", type: "bar", yAxisIndex: 0, itemStyle: { color: blue }, data: props.windows.map((w) => w.samples) },
            { name: "P95 (ms)", type: "line", yAxisIndex: 1, itemStyle: { color: warn }, data: props.windows.map((w) => toMs(w.p95_us)) },
            { name: "P99 (ms)", type: "line", yAxisIndex: 1, itemStyle: { color: err }, data: props.windows.map((w) => toMs(w.p99_us)) },
          ],
        },
        true,
      );
    }
  }
}

function handleResize(): void {
  for (const c of chartInstances.values()) c.resize();
}

watch(
  [() => props.series, () => props.breakdown, () => props.windows, () => props.timeline, corrMode, effectiveTheme],
  async () => {
    await nextTick();
    renderAll();
  },
  { deep: false },
);

onMounted(async () => {
  window.addEventListener("resize", handleResize);
  await nextTick();
  renderAll();
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", handleResize);
  for (const c of chartInstances.values()) c.dispose();
  chartInstances.clear();
});
</script>
