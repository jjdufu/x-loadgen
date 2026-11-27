<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 lg:p-8">
      <!-- Header -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-2.5">
          <UiButton
            variant="outline"
            size="icon-sm"
            aria-label="返回压测报告"
            @click="router.push({ name: 'runs' })"
          >
            <ArrowLeft class="h-4 w-4" />
          </UiButton>
          <h1 class="text-lg font-semibold tracking-tight text-foreground">
            报告对比
          </h1>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <UiSelect
            v-model="baseId"
            size="sm"
            wrapper-class="w-60"
            aria-label="基准报告"
          >
            <option value="">选择基准报告 (A)</option>
            <option v-for="r in runs" :key="r.id" :value="r.id">
              {{ r.title || r.id.slice(0, 8) }} ({{ formatTimeMs(r.started_at ?? r.created_at) }})
            </option>
          </UiSelect>

          <UiButton
            variant="ghost"
            size="icon-sm"
            title="互换"
            aria-label="互换基准与目标"
            @click="swapRuns"
          >
            <ArrowLeftRight class="h-4 w-4" />
          </UiButton>

          <UiSelect
            v-model="targetId"
            size="sm"
            wrapper-class="w-60"
            aria-label="目标报告"
          >
            <option value="">选择目标报告 (B)</option>
            <option v-for="r in runs" :key="r.id" :value="r.id">
              {{ r.title || r.id.slice(0, 8) }} ({{ formatTimeMs(r.started_at ?? r.created_at) }})
            </option>
          </UiSelect>

          <UiButton
            variant="default"
            size="sm"
            :disabled="!baseId || !targetId || loading"
            @click="runCompare"
          >
            对比
          </UiButton>
        </div>
      </div>

      <UiCard
        v-if="loading"
        class="flex items-center justify-center gap-2 py-20 text-sm text-muted-foreground"
      >
        <Loader2 class="h-4 w-4 animate-spin" />
        <span>计算对比中…</span>
      </UiCard>

      <UiCard
        v-else-if="!data"
        class="py-16 text-center text-sm text-muted-foreground"
      >
        选择两份压测报告开始对比
      </UiCard>

      <template v-else>
        <!-- Config Diff Summary -->
        <UiCard class="overflow-hidden">
          <div class="border-b border-border bg-muted/40 px-4 py-2.5 text-xs font-semibold text-foreground">
            配置对比
          </div>
          <div class="overflow-x-auto">
            <table class="w-full border-collapse text-left font-mono text-xs">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="px-4 py-2.5 font-medium">项目</th>
                  <th class="px-4 py-2.5 font-medium">基准 (A) · {{ data.base.title || data.base.id.slice(0, 8) }}</th>
                  <th class="px-4 py-2.5 font-medium">目标 (B) · {{ data.target.title || data.target.id.slice(0, 8) }}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr
                  v-for="row in configRows"
                  :key="row.label"
                  :class="row.changed ? 'bg-amber-500/5' : ''"
                >
                  <td class="px-4 py-2 text-muted-foreground">{{ row.label }}</td>
                  <td class="px-4 py-2 text-foreground">{{ row.base }}</td>
                  <td class="px-4 py-2 text-foreground">{{ row.target }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </UiCard>

        <!-- Core Metrics Delta Table -->
        <UiCard class="overflow-hidden">
          <div class="border-b border-border bg-muted/40 px-4 py-2.5 text-xs font-semibold text-foreground">
            核心指标变化
          </div>
          <div class="overflow-x-auto">
            <table class="w-full border-collapse text-left font-mono text-xs tabular-nums">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="px-4 py-2.5 font-medium">指标</th>
                  <th class="px-4 py-2.5 font-medium">基准 (A)</th>
                  <th class="px-4 py-2.5 font-medium">目标 (B)</th>
                  <th class="px-4 py-2.5 font-medium">差值</th>
                  <th class="px-4 py-2.5 font-medium">变化幅度</th>
                  <th class="px-4 py-2.5 font-medium">结论</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr v-for="m in data.metrics" :key="m.key">
                  <td class="px-4 py-2.5 font-sans font-medium text-foreground">{{ m.label }}</td>
                  <td class="px-4 py-2.5">{{ fmtMetricVal(m.base, m.unit) }}</td>
                  <td class="px-4 py-2.5">{{ fmtMetricVal(m.target, m.unit) }}</td>
                  <td class="px-4 py-2.5">{{ fmtMetricDelta(m.delta, m.unit) }}</td>
                  <td class="px-4 py-2.5" :class="statusColor(m.status)">
                    {{ m.delta_pct != null ? `${m.delta_pct > 0 ? "+" : ""}${m.delta_pct.toFixed(2)}%` : "-" }}
                  </td>
                  <td class="px-4 py-2.5">
                    <UiBadge
                      :variant="
                        m.status === 'improved'
                          ? 'success'
                          : m.status === 'regressed'
                            ? 'destructive'
                            : 'muted'
                      "
                    >
                      {{ m.status === "improved" ? "提升" : m.status === "regressed" ? "退化" : "持平" }}
                    </UiBadge>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </UiCard>

        <!-- Overlay Charts -->
        <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
          <UiCard class="p-5">
            <h2 class="mb-3 text-sm font-semibold text-foreground">延迟分位对比 (ms)</h2>
            <div ref="pctChartEl" class="h-72 w-full" />
          </UiCard>
          <UiCard class="p-5">
            <h2 class="mb-3 text-sm font-semibold text-foreground">吞吐趋势叠加 (相对秒)</h2>
            <div ref="qpsChartEl" class="h-72 w-full" />
          </UiCard>
          <UiCard class="p-5 lg:col-span-2">
            <h2 class="mb-3 text-sm font-semibold text-foreground">P95 / P99 延迟趋势叠加 (相对秒)</h2>
            <div ref="latChartEl" class="h-72 w-full" />
          </UiCard>
        </div>

        <!-- Per-Endpoint Diff Table -->
        <UiCard v-if="data.products_diff.length > 0" class="overflow-hidden">
          <div class="border-b border-border bg-muted/40 px-4 py-2.5 text-xs font-semibold text-foreground">
            接口分项对比
          </div>
          <div class="overflow-x-auto">
            <table class="w-full border-collapse text-left font-mono text-xs tabular-nums">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="px-4 py-2.5 font-medium">接口</th>
                  <th class="px-3 py-2.5 font-medium">A QPS</th>
                  <th class="px-3 py-2.5 font-medium">B QPS</th>
                  <th class="px-3 py-2.5 font-medium">A P95</th>
                  <th class="px-3 py-2.5 font-medium">B P95</th>
                  <th class="px-3 py-2.5 font-medium">P95 变化</th>
                  <th class="px-3 py-2.5 font-medium">A 错误率</th>
                  <th class="px-3 py-2.5 font-medium">B 错误率</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr v-for="p in data.products_diff" :key="p.product_id">
                  <td class="px-4 py-2.5 text-foreground">{{ p.product_id }}</td>
                  <td class="px-3 py-2.5">{{ p.base_qps?.toFixed(1) ?? "-" }}</td>
                  <td class="px-3 py-2.5">{{ p.target_qps?.toFixed(1) ?? "-" }}</td>
                  <td class="px-3 py-2.5">{{ formatLatencyUs(p.base_p95_us) }}</td>
                  <td class="px-3 py-2.5">{{ formatLatencyUs(p.target_p95_us) }}</td>
                  <td
                    class="px-3 py-2.5"
                    :class="
                      p.p95_delta_pct != null && p.p95_delta_pct < 0
                        ? 'text-emerald-600 dark:text-emerald-400'
                        : p.p95_delta_pct != null && p.p95_delta_pct > 0
                          ? 'text-rose-600 dark:text-rose-400'
                          : ''
                    "
                  >
                    {{ p.p95_delta_pct != null ? `${p.p95_delta_pct > 0 ? "+" : ""}${p.p95_delta_pct.toFixed(1)}%` : "-" }}
                  </td>
                  <td class="px-3 py-2.5">{{ p.base_error_rate != null ? `${(p.base_error_rate * 100).toFixed(2)}%` : "-" }}</td>
                  <td class="px-3 py-2.5">{{ p.target_error_rate != null ? `${(p.target_error_rate * 100).toFixed(2)}%` : "-" }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </UiCard>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { init as initChart, use as useECharts, type EChartsType } from "echarts/core";
import { BarChart, LineChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { ArrowLeft, ArrowLeftRight, Loader2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { CompareReportPayload, RunView } from "../api/types";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import { formatBytes, formatLatencyUs, formatTimeMs } from "../helpers/format";
import { effectiveTheme } from "../modules/theme";

useECharts([BarChart, LineChart, GridComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

const route = useRoute();
const router = useRouter();
const runs = ref<RunView[]>([]);
const baseId = ref(String(route.query.base ?? ""));
const targetId = ref(String(route.query.target ?? ""));
const loading = ref(false);
const data = ref<CompareReportPayload | null>(null);

const pctChartEl = ref<HTMLDivElement | null>(null);
const qpsChartEl = ref<HTMLDivElement | null>(null);
const latChartEl = ref<HTMLDivElement | null>(null);
let charts: EChartsType[] = [];

const configRows = computed(() => {
  if (!data.value) return [];
  const b = data.value.config_diff.base;
  const t = data.value.config_diff.target;
  const fmt = (v: unknown) => (v === null || v === undefined || v === "" ? "-" : Array.isArray(v) ? v.join(", ") : String(v));
  const defs: [string, string][] = [
    ["施压模型", "model"],
    ["目标 QPS", "qps"],
    ["并发上限", "concurrency"],
    ["进程数", "processes"],
    ["执行模式", "mode"],
    ["环境", "environment_name"],
    ["协议", "protocols"],
  ];
  return defs.map(([label, key]) => {
    const bv = fmt(b[key]);
    const tv = fmt(t[key]);
    return { label, base: bv, target: tv, changed: bv !== tv };
  });
});

function fmtMetricVal(val: number | null, unit: string): string {
  if (val === null || val === undefined) return "-";
  if (unit === "us") return formatLatencyUs(val);
  if (unit === "B/s") return `${formatBytes(val)}/s`;
  if (unit === "%") return `${val.toFixed(2)}%`;
  if (unit === "req/s") return `${val.toFixed(1)} req/s`;
  return val.toFixed(3);
}

function fmtMetricDelta(val: number | null, unit: string): string {
  if (val === null || val === undefined) return "-";
  const sign = val > 0 ? "+" : "";
  if (unit === "us") return `${sign}${formatLatencyUs(Math.abs(val))}`;
  if (unit === "B/s") return `${sign}${formatBytes(Math.abs(val))}/s`;
  if (unit === "%") return `${sign}${val.toFixed(2)}%`;
  if (unit === "req/s") return `${sign}${val.toFixed(1)}`;
  return `${sign}${val.toFixed(3)}`;
}

function statusColor(status: string): string {
  if (status === "improved") return "text-emerald-600 dark:text-emerald-400";
  if (status === "regressed") return "text-rose-600 dark:text-rose-400";
  return "text-muted-foreground";
}

function swapRuns(): void {
  const tmp = baseId.value;
  baseId.value = targetId.value;
  targetId.value = tmp;
  if (baseId.value && targetId.value) void runCompare();
}

async function runCompare(): Promise<void> {
  if (!baseId.value || !targetId.value) return;
  void router.replace({ query: { base: baseId.value, target: targetId.value } });
  loading.value = true;
  try {
    data.value = await api.compareReports(baseId.value, targetId.value);
    await nextTick();
    renderCharts();
  } finally {
    loading.value = false;
  }
}

function disposeCharts(): void {
  for (const c of charts) c.dispose();
  charts = [];
}

function renderCharts(): void {
  disposeCharts();
  if (!data.value) return;
  const isDark = effectiveTheme.value === "black";
  const axisColor = isDark ? "#a1a1aa" : "#71717a";
  const gridColor = isDark ? "rgba(255,255,255,0.08)" : "rgba(9,9,11,0.07)";
  const colA = isDark ? "#60a5fa" : "#2563eb";
  const colB = isDark ? "#34d399" : "#059669";
  const colA2 = isDark ? "#a78bfa" : "#7c3aed";
  const colB2 = isDark ? "#fb7185" : "#e11d48";
  const toMs = (v: number | null | undefined) => (v == null ? null : Number((v / 1000).toFixed(2)));
  const tooltip = {
    trigger: "axis" as const,
    backgroundColor: isDark ? "#111114" : "#ffffff",
    borderColor: isDark ? "#27272a" : "#e4e4e7",
    textStyle: { color: isDark ? "#fafafa" : "#09090b", fontSize: 12 },
  };

  if (pctChartEl.value) {
    const c = initChart(pctChartEl.value);
    charts.push(c);
    const bp = data.value.base.percentiles;
    const tp = data.value.target.percentiles;
    c.setOption({
      backgroundColor: "transparent",
      tooltip,
      grid: { left: 48, right: 16, top: 34, bottom: 26 },
      legend: { data: ["基准 (A)", "目标 (B)"], textStyle: { color: axisColor, fontSize: 12 } },
      xAxis: {
        type: "category",
        data: ["Avg", "P50", "P75", "P90", "P95", "P99", "Max"],
        axisLabel: { color: axisColor },
        axisLine: { lineStyle: { color: gridColor } },
      },
      yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor } },
      series: [
        {
          name: "基准 (A)",
          type: "bar",
          itemStyle: { color: colA },
          data: [bp.avg_us, bp.p50_us, bp.p75_us, bp.p90_us, bp.p95_us, bp.p99_us, bp.max_us].map(toMs),
        },
        {
          name: "目标 (B)",
          type: "bar",
          itemStyle: { color: colB },
          data: [tp.avg_us, tp.p50_us, tp.p75_us, tp.p90_us, tp.p95_us, tp.p99_us, tp.max_us].map(toMs),
        },
      ],
    });
  }

  const secs = data.value.aligned_series.map((p) => `${p.offset_sec}s`);
  if (qpsChartEl.value) {
    const c = initChart(qpsChartEl.value);
    charts.push(c);
    c.setOption({
      backgroundColor: "transparent",
      tooltip,
      grid: { left: 48, right: 16, top: 34, bottom: 26 },
      legend: { data: ["基准 QPS (A)", "目标 QPS (B)"], textStyle: { color: axisColor, fontSize: 12 } },
      xAxis: { type: "category", data: secs, axisLabel: { color: axisColor }, axisLine: { lineStyle: { color: gridColor } } },
      yAxis: { type: "value", name: "req/s", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor } },
      series: [
        { name: "基准 QPS (A)", type: "line", showSymbol: false, itemStyle: { color: colA }, data: data.value.aligned_series.map((p) => p.base_qps) },
        { name: "目标 QPS (B)", type: "line", showSymbol: false, itemStyle: { color: colB }, data: data.value.aligned_series.map((p) => p.target_qps) },
      ],
    });
  }

  if (latChartEl.value) {
    const c = initChart(latChartEl.value);
    charts.push(c);
    c.setOption({
      backgroundColor: "transparent",
      tooltip,
      grid: { left: 48, right: 16, top: 34, bottom: 26 },
      legend: { data: ["A P95", "B P95", "A P99", "B P99"], textStyle: { color: axisColor, fontSize: 12 } },
      xAxis: { type: "category", data: secs, axisLabel: { color: axisColor }, axisLine: { lineStyle: { color: gridColor } } },
      yAxis: { type: "value", name: "ms", splitLine: { lineStyle: { color: gridColor } }, axisLabel: { color: axisColor } },
      series: [
        { name: "A P95", type: "line", showSymbol: false, lineStyle: { type: "dashed" }, itemStyle: { color: colA }, data: data.value.aligned_series.map((p) => toMs(p.base_p95_us)) },
        { name: "B P95", type: "line", showSymbol: false, itemStyle: { color: colB }, data: data.value.aligned_series.map((p) => toMs(p.target_p95_us)) },
        { name: "A P99", type: "line", showSymbol: false, lineStyle: { type: "dashed" }, itemStyle: { color: colA2 }, data: data.value.aligned_series.map((p) => toMs(p.base_p99_us)) },
        { name: "B P99", type: "line", showSymbol: false, itemStyle: { color: colB2 }, data: data.value.aligned_series.map((p) => toMs(p.target_p99_us)) },
      ],
    });
  }
}

function handleResize(): void {
  for (const c of charts) c.resize();
}

watch(effectiveTheme, () => {
  renderCharts();
});

onMounted(async () => {
  window.addEventListener("resize", handleResize);
  runs.value = await api.listRuns();
  if (baseId.value && targetId.value) {
    await runCompare();
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", handleResize);
  disposeCharts();
});
</script>
