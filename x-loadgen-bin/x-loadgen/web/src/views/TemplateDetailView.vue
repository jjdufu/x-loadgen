<template>
  <div>
    <div class="page-header">
      <h2>{{ isNew ? "新建模板" : "编辑模板" }}</h2>
      <el-button @click="router.push({ name: 'templates' })">返回</el-button>
    </div>
    <el-form label-width="160px">
      <el-form-item label="名称" required>
        <el-input v-model="form.name" />
      </el-form-item>
      <el-form-item label="目标" required>
        <el-select v-model="form.target_id" filterable style="width: 360px">
          <el-option v-for="item in targets" :key="item.id" :label="item.name" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="模型">
        <el-select v-model="form.model">
          <el-option label="open_loop" value="open_loop" />
          <el-option label="closed_loop" value="closed_loop" />
        </el-select>
      </el-form-item>
      <el-form-item label="QPS">
        <el-input-number v-model="form.qps" :min="0" />
      </el-form-item>
      <el-form-item label="并发">
        <el-input-number v-model="form.concurrency" :min="1" />
      </el-form-item>
      <el-form-item label="请求数">
        <el-input-number v-model="form.requests" :min="0" />
      </el-form-item>
      <el-form-item label="时长（秒）">
        <el-input-number v-model="form.duration_secs" :min="0" />
      </el-form-item>
      <el-form-item label="预热（秒）">
        <el-input-number v-model="form.warmup_secs" :min="0" />
      </el-form-item>
      <el-form-item label="爬升（秒）">
        <el-input-number v-model="form.ramp_secs" :min="0" />
      </el-form-item>
      <el-form-item label="超时（秒）">
        <el-input-number v-model="form.timeout_secs" :min="1" />
      </el-form-item>
      <el-form-item label="连接超时（秒）">
        <el-input-number v-model="form.connect_timeout_secs" :min="1" />
      </el-form-item>
      <el-form-item label="进程数">
        <el-input-number v-model="form.processes" :min="1" />
      </el-form-item>
      <el-form-item label="QPS 分配">
        <el-select v-model="form.qps_mode">
          <el-option label="total" value="total" />
          <el-option label="per_process" value="per_process" />
        </el-select>
      </el-form-item>
      <el-form-item label="请求数分配">
        <el-select v-model="form.requests_mode">
          <el-option label="total" value="total" />
          <el-option label="per_process" value="per_process" />
        </el-select>
      </el-form-item>
      <el-form-item label="循环压测">
        <el-switch v-model="form.loop_enabled" />
      </el-form-item>
      <el-form-item label="轮次（空为无限）">
        <el-input-number v-model="form.rounds" :min="0" />
      </el-form-item>
      <el-form-item label="窗间隔（秒）">
        <el-input-number v-model="form.loop_interval_secs" :min="0" />
      </el-form-item>
      <el-form-item label="明细保留天数">
        <el-input-number v-model="form.detail_retain_days" :min="0" />
      </el-form-item>
      <el-form-item label="明细保留行数">
        <el-input-number v-model="form.detail_retain_rows" :min="0" />
      </el-form-item>
      <el-form-item label="携带 Request-ID">
        <el-switch v-model="form.include_request_id" />
      </el-form-item>
      <el-form-item label="备注">
        <el-input v-model="form.note" type="textarea" :rows="2" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" :loading="saving" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import { http } from "../api/http";
import type { TargetSummary, Template } from "../api/types";

interface TemplateForm {
  name: string;
  target_id: string;
  model: string;
  qps: number | undefined;
  concurrency: number;
  requests: number | undefined;
  duration_secs: number | undefined;
  warmup_secs: number;
  ramp_secs: number;
  timeout_secs: number;
  connect_timeout_secs: number;
  processes: number;
  qps_mode: string;
  requests_mode: string;
  loop_enabled: boolean;
  rounds: number | undefined;
  loop_interval_secs: number;
  detail_retain_days: number | undefined;
  detail_retain_rows: number | undefined;
  include_request_id: boolean;
  note: string;
}

const route = useRoute();
const router = useRouter();
const isNew = computed(() => route.name === "template-new");
const saving = ref(false);
const targets = ref<TargetSummary[]>([]);
const form = reactive<TemplateForm>({
  name: "",
  target_id: "",
  model: "open_loop",
  qps: 10,
  concurrency: 2,
  requests: 20,
  duration_secs: undefined,
  warmup_secs: 0,
  ramp_secs: 0,
  timeout_secs: 5,
  connect_timeout_secs: 5,
  processes: 1,
  qps_mode: "total",
  requests_mode: "total",
  loop_enabled: false,
  rounds: undefined,
  loop_interval_secs: 0,
  detail_retain_days: undefined,
  detail_retain_rows: undefined,
  include_request_id: true,
  note: "",
});

function asPayload(): Record<string, unknown> {
  return {
    name: form.name,
    target_id: form.target_id,
    model: form.model,
    qps: form.qps || null,
    concurrency: form.concurrency,
    requests: form.requests || null,
    duration_secs: form.duration_secs || null,
    warmup_secs: form.warmup_secs,
    ramp_secs: form.ramp_secs,
    timeout_secs: form.timeout_secs,
    connect_timeout_secs: form.connect_timeout_secs,
    processes: form.processes,
    qps_mode: form.qps_mode,
    requests_mode: form.requests_mode,
    loop_enabled: form.loop_enabled,
    rounds: form.rounds || null,
    loop_interval_secs: form.loop_interval_secs,
    detail_retain_days: form.detail_retain_days || null,
    detail_retain_rows: form.detail_retain_rows || null,
    include_request_id: form.include_request_id,
    note: form.note || null,
  };
}

function fill(tpl: Template): void {
  form.name = tpl.name;
  form.target_id = tpl.target_id;
  form.model = tpl.model;
  form.qps = tpl.qps ?? undefined;
  form.concurrency = tpl.concurrency;
  form.requests = tpl.requests ?? undefined;
  form.duration_secs = tpl.duration_secs ?? undefined;
  form.warmup_secs = tpl.warmup_secs;
  form.ramp_secs = tpl.ramp_secs;
  form.timeout_secs = tpl.timeout_secs;
  form.connect_timeout_secs = tpl.connect_timeout_secs;
  form.processes = tpl.processes;
  form.qps_mode = tpl.qps_mode;
  form.requests_mode = tpl.requests_mode;
  form.loop_enabled = tpl.loop_enabled;
  form.rounds = tpl.rounds ?? undefined;
  form.loop_interval_secs = tpl.loop_interval_secs;
  form.detail_retain_days = tpl.detail_retain_days ?? undefined;
  form.detail_retain_rows = tpl.detail_retain_rows ?? undefined;
  form.include_request_id = tpl.include_request_id;
  form.note = tpl.note ?? "";
}

async function load(): Promise<void> {
  const list = await http.get<TargetSummary[]>("/api/v1/targets");
  targets.value = list.data;
  if (isNew.value) return;
  const { data } = await http.get<Template>(`/api/v1/templates/${String(route.params.id)}`);
  fill(data);
}

async function save(): Promise<void> {
  saving.value = true;
  try {
    if (isNew.value) {
      const { data } = await http.post<Template>("/api/v1/templates", asPayload());
      ElMessage.success("已创建");
      await router.push({ name: "template-detail", params: { id: data.id } });
    } else {
      await http.put(`/api/v1/templates/${String(route.params.id)}`, asPayload());
      ElMessage.success("已保存");
    }
  } finally {
    saving.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>
