<template>
  <div>
    <div class="page-header">
      <h2>报告 {{ report?.id ?? "" }}</h2>
      <div>
        <el-button @click="router.push({ name: 'run-detail', params: { id } })">返回任务</el-button>
        <el-button type="primary" @click="download('json')">下载 JSON</el-button>
        <el-button type="primary" @click="download('html')">下载 HTML</el-button>
      </div>
    </div>
    <template v-if="report">
      <p>状态：{{ report.status }} · 模式：{{ report.mode }} · 进程：{{ report.processes }}</p>
      <p v-if="report.verdict" class="verdict-line" :class="report.verdict.result">
        判定：{{ report.verdict.result }}
        <span v-if="failedChecks.length">（未通过：{{ failedChecks }}）</span>
      </p>
      <div class="card-grid">
        <div class="stat-card">
          <div class="label">样本</div>
          <div class="value">{{ report.counters.samples }}</div>
        </div>
        <div class="stat-card">
          <div class="label">成功</div>
          <div class="value">{{ report.counters.success }}</div>
        </div>
        <div class="stat-card">
          <div class="label">失败</div>
          <div class="value">{{ report.counters.failure }}</div>
        </div>
        <div class="stat-card">
          <div class="label">p99 (µs)</div>
          <div class="value">{{ report.percentiles.p99_us ?? "-" }}</div>
        </div>
      </div>
      <h3>窗口</h3>
      <el-table :data="report.windows" size="small" stripe>
        <el-table-column prop="window_index" label="#" width="60" />
        <el-table-column prop="samples" label="样本" />
        <el-table-column prop="success" label="成功" />
        <el-table-column prop="failure" label="失败" />
        <el-table-column prop="p99_us" label="p99 (µs)" />
      </el-table>
      <h3>时间线</h3>
      <el-table :data="report.timeline" size="small" stripe>
        <el-table-column prop="ts" label="时间" />
        <el-table-column prop="event" label="事件" />
      </el-table>
      <h3>中断段</h3>
      <el-table v-if="(report.interrupt_ranges ?? []).length" :data="report.interrupt_ranges ?? []" size="small">
        <el-table-column prop="start" label="开始" />
        <el-table-column prop="end" label="结束" />
      </el-table>
      <p v-else class="muted">无中断段</p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import { http } from "../api/http";
import type { ReportPayload } from "../api/types";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id));
const report = ref<ReportPayload | null>(null);
const failedChecks = computed(() =>
  (report.value?.verdict?.failed ?? []).map((c) => c.check).join(", "),
);

async function load(): Promise<void> {
  const { data } = await http.get<ReportPayload>(`/api/v1/runs/${id.value}/report`);
  report.value = data;
  const text = JSON.stringify(data);
  if (
    text.includes("api_key") ||
    text.includes("client_key") ||
    text.includes("binding_key") ||
    text.includes("tenant_key_base64")
  ) {
    ElMessage.warning("报告中不应出现密钥字段名，请检查脱敏");
  }
}

async function download(kind: "json" | "html"): Promise<void> {
  const { data } = await http.get<Blob>(`/api/v1/runs/${id.value}/report.${kind}`, {
    responseType: "blob",
  });
  const url = URL.createObjectURL(data);
  const link = document.createElement("a");
  link.href = url;
  link.download = `run-${id.value}-report.${kind}`;
  link.click();
  URL.revokeObjectURL(url);
}

onMounted(() => {
  void load();
});
</script>

<style scoped>
.muted {
  color: #6b7280;
}
.verdict-line.pass {
  color: #067d37;
}
.verdict-line.fail {
  color: #b42318;
}
.verdict-line.pending {
  color: #b54708;
}
</style>
