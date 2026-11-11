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
import type { RunView, Template } from "../api/types";

const router = useRouter();
const templates = ref<Template[]>([]);
const rows = ref<RunView[]>([]);
const templateId = ref("");
const mode = ref("once");
const starting = ref(false);

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
    const { data } = await http.post<RunView>("/api/v1/runs", {
      template_id: templateId.value,
      mode: mode.value,
    });
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
</style>
