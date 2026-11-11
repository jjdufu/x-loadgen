<template>
  <el-container class="layout">
    <el-aside width="200px" class="aside">
      <div class="brand">x-loadgen</div>
      <el-menu :default-active="active" router>
        <el-menu-item index="/targets">目标</el-menu-item>
        <el-menu-item index="/templates">模板</el-menu-item>
        <el-menu-item index="/runs">任务</el-menu-item>
        <el-menu-item index="/settings">设置</el-menu-item>
      </el-menu>
    </el-aside>
    <el-container>
      <el-header class="header">
        <span class="muted">{{ meta?.version ? `版本 ${meta.version}` : "" }}</span>
        <el-button v-if="authRequired" text @click="logout">退出登录</el-button>
      </el-header>
      <el-main>
        <router-view />
      </el-main>
    </el-container>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { http } from "../api/http";
import type { MeResponse, MetaResponse } from "../api/types";

const route = useRoute();
const router = useRouter();
const meta = ref<MetaResponse | null>(null);
const authRequired = ref(false);
const active = computed(() => {
  const path = route.path;
  if (path.startsWith("/targets")) return "/targets";
  if (path.startsWith("/templates")) return "/templates";
  if (path.startsWith("/settings")) return "/settings";
  return "/runs";
});

onMounted(async () => {
  const me = await http.get<MeResponse>("/api/v1/auth/me");
  authRequired.value = me.data.auth_required;
  try {
    const res = await http.get<MetaResponse>("/api/v1/meta");
    meta.value = res.data;
  } catch {
    meta.value = null;
  }
});

async function logout(): Promise<void> {
  await http.post("/api/v1/auth/logout");
  await router.push({ name: "login" });
}
</script>

<style scoped>
.layout {
  height: 100%;
}
.aside {
  background: #1d4ed8;
  color: #fff;
}
.brand {
  padding: 18px 16px 8px;
  font-weight: 700;
  font-size: 16px;
}
.aside :deep(.el-menu) {
  border-right: none;
  background: transparent;
  color: #fff;
}
.aside :deep(.el-menu-item) {
  color: #dbeafe;
}
.aside :deep(.el-menu-item.is-active),
.aside :deep(.el-menu-item:hover) {
  background: #1e40af;
  color: #fff;
}
.header {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  background: #fff;
  border-bottom: 1px solid #e5e7eb;
}
.muted {
  color: #6b7280;
  font-size: 13px;
}
</style>
