<template>
  <header
    class="grid grid-cols-3 items-center justify-between px-3 py-1.5 h-12 border-b border-dividerLight bg-primary select-none z-20"
  >
    <!-- Brand -->
    <div class="flex items-center space-x-2.5">
      <button
        type="button"
        class="flex items-center space-x-2 px-2 py-1 rounded hover:bg-primaryLight transition"
        @click="router.push({ name: 'workspace' })"
      >
        <span
          class="inline-flex items-center justify-center w-6 h-6 rounded bg-accent text-accentContrast font-bold text-tiny shadow-sm"
        >
          XL
        </span>
        <span class="font-bold tracking-wide text-secondaryDark uppercase">
          x-loadgen
        </span>
      </button>
      <span
        class="hidden md:inline-flex items-center px-2 py-0.5 rounded-full text-tiny font-mono border border-divider bg-primaryLight text-secondary"
      >
        HTTP · Mazbri Studio
      </span>
      <span
        v-if="serverMeta?.version"
        class="hidden lg:inline text-tiny font-mono text-secondaryLight"
      >
        v{{ serverMeta.version }}
      </span>
    </div>

    <!-- Center Quick Command / Capacity Bar -->
    <div class="flex items-center justify-center">
      <button
        type="button"
        class="flex items-center justify-between w-full max-w-xs px-3 py-1.5 rounded border border-dividerLight bg-primaryDark hover:bg-primaryLight text-secondary transition text-tiny"
        @click="shortcutsModalOpen = true"
      >
        <span class="flex items-center space-x-2 truncate">
          <Terminal class="w-3.5 h-3.5 text-accent flex-shrink-0" />
          <span class="truncate">快速操作与快捷键 · 发送 / 保存 / 压测</span>
        </span>
        <span class="shortcut-key">Ctrl /</span>
      </button>
    </div>

    <!-- Right Status & Actions -->
    <div class="flex items-center justify-end space-x-1.5">
      <button
        v-if="serverMeta"
        v-tippy="'点击查看实时压测任务列表'"
        type="button"
        class="hidden sm:inline-flex items-center space-x-2 px-2.5 py-1 rounded border border-dividerLight bg-primaryLight hover:border-accent transition text-tiny font-mono"
        @click="router.push({ name: 'runs' })"
      >
        <span
          class="w-2 h-2 rounded-full"
          :class="
            serverMeta.running_concurrency > 0 || serverMeta.running_qps > 0
              ? 'bg-emerald-500 animate-pulse'
              : 'bg-secondaryLight'
          "
        />
        <span class="text-secondaryDark">
          {{ serverMeta.running_qps }} QPS
        </span>
        <span class="text-dividerDark">|</span>
        <span class="text-secondary">
          {{ serverMeta.running_concurrency }} 并发
        </span>
      </button>

      <HoppButtonSecondary
        v-tippy="`切换背景主题 (当前: ${bgColor})`"
        :icon="effectiveColorMode === 'light' ? Sun : Moon"
        @click="cycleColorMode"
      />

      <HoppButtonSecondary
        v-tippy="'OpenAPI 3.1 接口规范 (/api/v1/openapi.json)'"
        :icon="FileJson"
        to="/api/v1/openapi.json"
        blank
      />

      <HoppButtonSecondary
        v-if="serverMeta?.auth_required"
        v-tippy="'退出登录'"
        :icon="LogOut"
        @click="handleLogout"
      />
    </div>
  </header>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import { HoppButtonSecondary } from "@hoppscotch/ui";
import { FileJson, LogOut, Moon, Sun, Terminal } from "lucide-vue-next";
import { api } from "../../api/workspace";
import {
  bgColor,
  cycleColorMode,
  effectiveColorMode,
} from "../../modules/theming";
import { toast } from "../../modules/toast";
import { serverMeta, shortcutsModalOpen } from "../../stores/workspace";

const router = useRouter();

async function handleLogout(): Promise<void> {
  await api.logout();
  toast.info("已安全退出");
  await router.push({ name: "login" });
}
</script>
