<template>
  <div>
    <div class="page-header">
      <h2>目标</h2>
      <el-button type="primary" @click="router.push({ name: 'target-new' })">新建目标</el-button>
    </div>
    <el-table :data="rows" stripe>
      <el-table-column prop="name" label="名称" />
      <el-table-column prop="gateway_base_url" label="网关地址" />
      <el-table-column label="跳过校验证书" width="140">
        <template #default="{ row }">{{ row.insecure_skip_verify ? "是" : "否" }}</template>
      </el-table-column>
      <el-table-column label="操作" width="180">
        <template #default="{ row }">
          <el-button text type="primary" @click="router.push({ name: 'target-detail', params: { id: row.id } })">
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
import type { TargetSummary } from "../api/types";

const router = useRouter();
const rows = ref<TargetSummary[]>([]);

async function load(): Promise<void> {
  const { data } = await http.get<TargetSummary[]>("/api/v1/targets");
  rows.value = data;
}

async function remove(id: string): Promise<void> {
  try {
    await ElMessageBox.confirm("删除该目标及其租户和产品？", "确认删除", {
      confirmButtonText: "删除",
      cancelButtonText: "取消",
      type: "warning",
    });
  } catch {
    return;
  }
  await http.delete(`/api/v1/targets/${id}`);
  ElMessage.success("已删除");
  await load();
}

onMounted(() => {
  void load();
});
</script>
