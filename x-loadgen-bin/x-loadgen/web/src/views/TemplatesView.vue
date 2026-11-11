<template>
  <div>
    <div class="page-header">
      <h2>模板</h2>
      <el-button type="primary" @click="router.push({ name: 'template-new' })">新建模板</el-button>
    </div>
    <el-table :data="rows" stripe>
      <el-table-column prop="name" label="名称" />
      <el-table-column prop="model" label="模型" width="120" />
      <el-table-column prop="qps" label="QPS" width="80" />
      <el-table-column prop="concurrency" label="并发" width="80" />
      <el-table-column label="循环" width="80">
        <template #default="{ row }">{{ row.loop_enabled ? "是" : "否" }}</template>
      </el-table-column>
      <el-table-column prop="processes" label="进程数" width="90" />
      <el-table-column label="操作" width="180">
        <template #default="{ row }">
          <el-button text type="primary" @click="router.push({ name: 'template-detail', params: { id: row.id } })">
            编辑
          </el-button>
          <el-button text type="danger" @click="remove(row.id)">删除</el-button>
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
import type { Template } from "../api/types";

const router = useRouter();
const rows = ref<Template[]>([]);

async function load(): Promise<void> {
  const { data } = await http.get<Template[]>("/api/v1/templates");
  rows.value = data;
}

async function remove(id: string): Promise<void> {
  try {
    await ElMessageBox.confirm("删除该模板？", "确认删除", {
      confirmButtonText: "删除",
      cancelButtonText: "取消",
      type: "warning",
    });
  } catch {
    return;
  }
  await http.delete(`/api/v1/templates/${id}`);
  ElMessage.success("已删除");
  await load();
}

onMounted(() => {
  void load();
});
</script>
