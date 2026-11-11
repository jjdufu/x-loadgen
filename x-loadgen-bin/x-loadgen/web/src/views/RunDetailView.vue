<template>
  <div>
    <div class="page-header">
      <h2>任务 {{ run?.id ?? "" }}</h2>
      <div>
        <el-button @click="router.push({ name: 'runs' })">返回</el-button>
        <el-button @click="router.push({ name: 'run-report', params: { id: id } })">报告</el-button>
        <el-button v-if="canStop" type="warning" @click="stop">停止</el-button>
        <el-button v-if="canResume" type="primary" @click="resume">续跑</el-button>
      </div>
    </div>
    <p>状态：{{ run?.status }} · 模式：{{ run?.mode }} · 进程：{{ run?.processes }}</p>
    <div class="card-grid">
      <div v-for="card in cards" :key="card.label" class="stat-card">
        <div class="label">{{ card.label }}</div>
        <div class="value">{{ card.value }}</div>
      </div>
    </div>
    <el-form :inline="true" @submit.prevent="patchTraffic">
      <el-form-item label="QPS">
        <el-input-number v-model="qps" :min="1" />
      </el-form-item>
      <el-form-item label="并发">
        <el-input-number v-model="concurrency" :min="1" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" :disabled="!canStop" native-type="submit">调整流量</el-button>
      </el-form-item>
    </el-form>
    <div ref="chartEl" class="chart-box" />
    <h3>窗口</h3>
    <el-table :data="windows" size="small" stripe>
      <el-table-column prop="window_index" label="#" width="60" />
      <el-table-column prop="samples" label="样本" />
      <el-table-column prop="success" label="成功" />
      <el-table-column prop="failure" label="失败" />
      <el-table-column prop="expected" label="预期" />
      <el-table-column prop="p99_us" label="p99 (µs)" />
    </el-table>
    <h3>时间线</h3>
    <el-table :data="timeline" size="small" stripe>
      <el-table-column prop="ts" label="时间" />
      <el-table-column prop="event" label="事件" />
    </el-table>
    <h3>中断段</h3>
    <el-table v-if="(run?.interrupt_ranges ?? []).length" :data="run?.interrupt_ranges ?? []" size="small">
      <el-table-column prop="start" label="开始" />
      <el-table-column prop="end" label="结束" />
    </el-table>
    <p v-else class="muted">无中断段</p>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import * as echarts from "echarts";
import { ElMessage } from "element-plus";
import { http } from "../api/http";
import type { RunStats, RunView, TimePoint, TimelineEvent, WindowRow } from "../api/types";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id));
const run = ref<RunView | null>(null);
const stats = ref<RunStats | null>(null);
const windows = ref<WindowRow[]>([]);
const timeline = ref<TimelineEvent[]>([]);
const qps = ref(1);
const concurrency = ref(1);
const chartEl = ref<HTMLDivElement | null>(null);
let chart: echarts.ECharts | null = null;
let source: EventSource | null = null;
let pollTimer: number | null = null;

const canStop = computed(() => run.value?.status === "running" || run.value?.status === "stopping");
const canResume = computed(() => run.value?.status === "stopped" || run.value?.status === "interrupted");

const cards = computed(() => {
  const s = stats.value;
  return [
    { label: "样本", value: s?.samples ?? "-" },
    { label: "成功", value: s?.success ?? "-" },
    { label: "失败", value: s?.failure ?? "-" },
    { label: "预期", value: s?.expected ?? "-" },
    { label: "p50 (µs)", value: s?.p50_us ?? "-" },
    { label: "p99 (µs)", value: s?.p99_us ?? "-" },
    { label: "进行中", value: s?.in_flight ?? "-" },
  ];
});

async function refresh(): Promise<void> {
  const [runRes, statsRes, winRes, timeRes, seriesRes] = await Promise.all([
    http.get<RunView>(`/api/v1/runs/${id.value}`),
    http.get<RunStats>(`/api/v1/runs/${id.value}/stats`),
    http.get<{ windows: WindowRow[] }>(`/api/v1/runs/${id.value}/windows`),
    http.get<{ events: TimelineEvent[] }>(`/api/v1/runs/${id.value}/timeline`),
    http.get<{ points: TimePoint[] }>(`/api/v1/runs/${id.value}/timeseries`),
  ]);
  run.value = runRes.data;
  stats.value = statsRes.data;
  windows.value = winRes.data.windows ?? [];
  timeline.value = timeRes.data.events ?? [];
  if (statsRes.data.current_qps) qps.value = statsRes.data.current_qps;
  if (statsRes.data.current_concurrency) concurrency.value = statsRes.data.current_concurrency;
  renderChart(seriesRes.data.points ?? []);
}

function renderChart(points: TimePoint[]): void {
  if (!chartEl.value) return;
  if (!chart) chart = echarts.init(chartEl.value);
  chart.setOption({
    tooltip: { trigger: "axis" },
    legend: { data: ["样本", "成功", "失败"] },
    xAxis: { type: "category", data: points.map((p) => p.unix_sec ?? p.started_at ?? "") },
    yAxis: { type: "value" },
    series: [
      { name: "样本", type: "line", data: points.map((p) => p.samples || 0) },
      { name: "成功", type: "line", data: points.map((p) => p.success || 0) },
      { name: "失败", type: "line", data: points.map((p) => p.failure || 0) },
    ],
  });
}

function startLive(): void {
  try {
    source = new EventSource(`/api/v1/runs/${id.value}/events`);
    source.addEventListener("stats", () => {
      void refresh();
    });
    source.onerror = () => {
      source?.close();
      source = null;
      startPoll();
    };
  } catch {
    startPoll();
  }
}

function startPoll(): void {
  if (pollTimer !== null) return;
  pollTimer = window.setInterval(() => {
    void refresh();
  }, 1000);
}

async function stop(): Promise<void> {
  await http.post(`/api/v1/runs/${id.value}/stop`);
  ElMessage.success("已请求停止");
  await refresh();
}

async function resume(): Promise<void> {
  await http.post(`/api/v1/runs/${id.value}/resume`);
  ElMessage.success("已续跑");
  await refresh();
}

async function patchTraffic(): Promise<void> {
  await http.patch(`/api/v1/runs/${id.value}/traffic`, {
    qps: qps.value,
    concurrency: concurrency.value,
  });
  ElMessage.success("已调整流量");
  await refresh();
}

onMounted(async () => {
  await refresh();
  startLive();
});

onUnmounted(() => {
  source?.close();
  if (pollTimer !== null) window.clearInterval(pollTimer);
  chart?.dispose();
});
</script>

<style scoped>
.muted {
  color: #6b7280;
}
h3 {
  margin: 20px 0 8px;
}
</style>
