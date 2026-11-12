<template>
  <div>
    <div class="page-header">
      <h2>任务</h2>
    </div>
    <el-form :inline="true" class="start-bar" @submit.prevent="startRun">
      <el-form-item label="模板">
        <el-select v-model="templateId" filterable placeholder="选择模板" style="width: 260px">
          <el-option v-for="item in templates" :key="item.id" :label="item.name" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="模式">
        <el-select v-model="mode" style="width: 120px">
          <el-option label="once" value="once" />
          <el-option label="loop" value="loop" />
        </el-select>
      </el-form-item>
      <el-form-item>
        <el-button type="primary" :loading="starting" native-type="submit">启动</el-button>
      </el-form-item>
      <div class="assert-row">
        <span class="assert-hint">可选断言（结束后判定）</span>
        <el-form-item label="p50 ms">
          <el-input-number v-model="assertP50" :min="0.001" :controls="false" :precision="3" placeholder="可选" />
        </el-form-item>
        <el-form-item label="p95 ms">
          <el-input-number v-model="assertP95" :min="0.001" :controls="false" :precision="3" placeholder="可选" />
        </el-form-item>
        <el-form-item label="p99 ms">
          <el-input-number v-model="assertP99" :min="0.001" :controls="false" :precision="3" placeholder="可选" />
        </el-form-item>
        <el-form-item label="max ms">
          <el-input-number v-model="assertMax" :min="0.001" :controls="false" :precision="3" placeholder="可选" />
        </el-form-item>
        <el-form-item label="最大错误率">
          <el-input-number v-model="assertErrorRate" :min="0" :max="1" :step="0.01" :controls="false" :precision="4" placeholder="0–1" />
        </el-form-item>
        <el-form-item label="最低 QPS 比">
          <el-input-number v-model="assertQpsRatio" :min="0.001" :controls="false" :precision="3" placeholder="可选" />
        </el-form-item>
      </div>
    </el-form>
    <el-table :data="rows" stripe>
      <el-table-column prop="id" label="ID" min-width="180" />
      <el-table-column prop="status" label="状态" width="110" />
      <el-table-column prop="mode" label="模式" width="80" />
      <el-table-column prop="current_qps" label="QPS" width="80" />
      <el-table-column prop="current_concurrency" label="并发" width="80" />
      <el-table-column prop="processes" label="进程" width="80" />
      <el-table-column label="操作" width="160">
        <template #default="{ row }">
          <el-button text type="primary" @click="router.push({ name: 'run-detail', params: { id: row.id } })">
            详情
          </el-button>
          <el-button text type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { ElMessage, ElMessageBox } from "element-plus";
import { http } from "../api/http";
import type { AssertSpec, RunView, Template } from "../api/types";

const router = useRouter();
const templates = ref<Template[]>([]);
const rows = ref<RunView[]>([]);
const templateId = ref("");
const mode = ref("once");
const starting = ref(false);
const assertP50 = ref<number | undefined>(undefined);
const assertP95 = ref<number | undefined>(undefined);
const assertP99 = ref<number | undefined>(undefined);
const assertMax = ref<number | undefined>(undefined);
const assertErrorRate = ref<number | undefined>(undefined);
const assertQpsRatio = ref<number | undefined>(undefined);

function buildAssert(): AssertSpec | undefined {
  const spec: AssertSpec = {};
  if (assertP50.value != null) spec.p50_ms = assertP50.value;
  if (assertP95.value != null) spec.p95_ms = assertP95.value;
  if (assertP99.value != null) spec.p99_ms = assertP99.value;
  if (assertMax.value != null) spec.max_ms = assertMax.value;
  if (assertErrorRate.value != null) spec.max_error_rate = assertErrorRate.value;
  if (assertQpsRatio.value != null) spec.min_qps_ratio = assertQpsRatio.value;
  return Object.keys(spec).length > 0 ? spec : undefined;
}

async function load(): Promise<void> {
  const [tpl, runs] = await Promise.all([
    http.get<Template[]>("/api/v1/templates"),
    http.get<RunView[]>("/api/v1/runs"),
  ]);
  templates.value = tpl.data;
  rows.value = runs.data;
}

async function startRun(): Promise<void> {
  if (!templateId.value) {
    ElMessage.error("请选择模板");
    return;
  }
  starting.value = true;
  try {
    const { data: running } = await http.get<RunView[]>("/api/v1/runs", {
      params: { status: "running" },
    });
    const count = running.length;
    if (count > 0) {
      try {
        await ElMessageBox.confirm(
          `当前已有 ${count} 个任务在运行，同时压测可能互相影响结果。是否继续启动？`,
          "同时运行确认",
          {
            confirmButtonText: "继续启动",
            cancelButtonText: "取消",
            type: "warning",
            autofocus: false,
          },
        );
      } catch {
        return;
      }
    }
    const body: { template_id: string; mode: string; assert?: AssertSpec } = {
      template_id: templateId.value,
      mode: mode.value,
    };
    const assertSpec = buildAssert();
    if (assertSpec) body.assert = assertSpec;
    const { data } = await http.post<RunView>("/api/v1/runs", body);
    ElMessage.success("任务已启动");
    await router.push({ name: "run-detail", params: { id: data.id } });
  } finally {
    starting.value = false;
  }
}

async function remove(row: RunView): Promise<void> {
  try {
    await ElMessageBox.confirm("删除该任务及其目录？运行中的任务需先停止。", "确认删除", {
      confirmButtonText: "删除",
      cancelButtonText: "取消",
      type: "warning",
    });
  } catch {
    return;
  }
  await http.delete(`/api/v1/runs/${row.id}`);
  ElMessage.success("已删除");
  await load();
}

onMounted(() => {
  void load();
});
</script>

<style scoped>
.start-bar {
  background: #fff;
  padding: 12px 16px 0;
  border-radius: 8px;
  margin-bottom: 16px;
}
.assert-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  width: 100%;
  padding-bottom: 8px;
}
.assert-hint {
  color: #6b7280;
  font-size: 12px;
  margin-right: 12px;
  margin-bottom: 18px;
}
</style>
