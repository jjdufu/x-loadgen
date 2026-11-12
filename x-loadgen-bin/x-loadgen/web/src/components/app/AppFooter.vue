<template>
  <footer
    class="flex h-8 shrink-0 items-center justify-between gap-2 border-t border-border bg-background px-3 text-xs text-muted-foreground select-none"
  >
    <div class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
      <button
        type="button"
        :title="leftSidebarExpanded ? '收起侧栏' : '展开侧栏'"
        aria-label="切换左侧导航栏"
        class="inline-flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded transition-colors hover:bg-accent hover:text-foreground"
        @click="leftSidebarExpanded = !leftSidebarExpanded"
      >
        <PanelLeft class="h-3.5 w-3.5" />
      </button>

      <button
        type="button"
        title="查看压测任务"
        class="inline-flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded px-1.5 font-mono text-[11px] tabular-nums transition-colors hover:bg-accent hover:text-foreground"
        @click="router.push({ name: 'runs' })"
      >
        <span
          class="h-1.5 w-1.5 shrink-0 rounded-full"
          :class="statusDotClass"
        />
        <template v-if="serverMeta">
          <span class="font-medium text-foreground">
            {{ serverMeta.running_qps }} QPS
          </span>
          <span class="text-border">·</span>
          <span>{{ serverMeta.running_concurrency }} 并发</span>
        </template>
        <span v-else>连接中</span>
      </button>

      <template v-if="isWorkspaceRoute">
        <span class="hidden sm:inline text-border">·</span>
        <span class="hidden sm:inline truncate text-[11px]">
          环境:
          <span class="text-foreground">
            {{ activeEnvironmentDetail?.name ?? "全局变量" }}
          </span>
        </span>

        <template v-if="activeTab.response">
          <span class="hidden md:inline text-border">·</span>
          <button
            type="button"
            title="展开或收起响应栏"
            class="hidden md:inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 font-mono text-[11px] tabular-nums transition-colors hover:bg-accent"
            @click="responsePaneOpen = !responsePaneOpen"
          >
            <span
              :class="
                activeTab.response.transport_ok &&
                  (activeTab.response.http_status ?? 0) > 0 &&
                  (activeTab.response.http_status ?? 0) < 400
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-amber-600 dark:text-amber-400'
              "
            >
              {{ activeTab.response.http_status || "异常" }}
            </span>
            <span class="text-border">·</span>
            <span class="text-muted-foreground">
              {{ formatLatencyUs(activeTab.response.latency_us) }}
            </span>
          </button>
        </template>
      </template>
    </div>

    <div v-if="isWorkspaceRoute" class="flex shrink-0 items-center gap-1">
      <button
        type="button"
        :title="responsePaneOpen ? '收起响应栏' : '展开响应栏'"
        aria-label="切换响应栏"
        class="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded transition-colors hover:bg-accent hover:text-foreground"
        :class="responsePaneOpen ? 'text-foreground' : 'text-muted-foreground'"
        @click="responsePaneOpen = !responsePaneOpen"
      >
        <PanelBottom class="h-3.5 w-3.5" />
      </button>

      <button
        type="button"
        :title="horizontalSplit ? '切换为上下分栏' : '切换为左右分栏'"
        aria-label="切换分栏方向"
        class="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded transition-colors hover:bg-accent hover:text-foreground"
        @click="horizontalSplit = !horizontalSplit"
      >
        <Columns2
          class="h-3.5 w-3.5 transition-transform"
          :class="{ 'rotate-90': !horizontalSplit }"
        />
      </button>

      <button
        type="button"
        :title="rightSidebarOpen ? '收起右侧栏' : '展开右侧栏'"
        aria-label="切换右侧栏"
        class="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded transition-colors hover:bg-accent hover:text-foreground"
        :class="rightSidebarOpen ? 'text-foreground' : 'text-muted-foreground'"
        @click="rightSidebarOpen = !rightSidebarOpen"
      >
        <PanelRight class="h-3.5 w-3.5" />
      </button>
    </div>
  </footer>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Columns2,
  PanelBottom,
  PanelLeft,
  PanelRight,
} from "lucide-vue-next";
import { formatLatencyUs } from "../../helpers/format";
import {
  activeEnvironmentDetail,
  activeTab,
  horizontalSplit,
  leftSidebarExpanded,
  responsePaneOpen,
  rightSidebarOpen,
  serverMeta,
} from "../../stores/workspace";

const route = useRoute();
const router = useRouter();
const isWorkspaceRoute = computed(() => route.name === "workspace");

const statusDotClass = computed(() => {
  if (!serverMeta.value) {
    return "bg-amber-500/70";
  }
  if (
    serverMeta.value.running_qps > 0 ||
    serverMeta.value.running_concurrency > 0
  ) {
    return "bg-emerald-500 animate-pulse";
  }
  return "bg-muted-foreground/45";
});
</script>
