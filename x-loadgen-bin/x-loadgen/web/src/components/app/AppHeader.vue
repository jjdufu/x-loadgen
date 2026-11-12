<template>
  <header
    class="z-20 flex h-12 shrink-0 items-center justify-between border-b border-border bg-background px-4 select-none"
  >
    <!-- Brand & Context Breadcrumb -->
    <div class="flex items-center gap-3 min-w-0">
      <button
        type="button"
        class="inline-flex items-center gap-2.5 rounded-md py-1 pr-2 text-left transition-opacity hover:opacity-80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        @click="router.push({ name: 'workspace' })"
      >
        <span
          class="inline-flex h-6 w-6 items-center justify-center rounded-md bg-foreground font-mono text-[11px] font-bold tracking-tighter text-background"
        >
          XL
        </span>
        <span class="text-sm font-semibold tracking-tight text-foreground">
          x-loadgen
        </span>
      </button>

      <span class="text-border select-none">/</span>

      <span class="truncate text-sm font-medium text-muted-foreground">
        {{ sectionTitle }}
      </span>

      <span
        v-if="serverMeta?.version"
        class="hidden sm:inline-flex items-center rounded-md border border-border bg-muted/50 px-2 py-0.5 font-mono text-[11px] text-muted-foreground tabular-nums"
      >
        v{{ serverMeta.version }}
      </span>
    </div>

    <!-- Right Status & Clean Monochrome Controls -->
    <div class="flex items-center gap-2 shrink-0">
      <button
        v-if="serverMeta"
        type="button"
        title="查看实时与历史压测任务"
        class="hidden sm:inline-flex h-8 items-center gap-2 rounded-md border border-border bg-muted/40 px-3 font-mono text-xs tabular-nums transition-colors hover:bg-accent hover:text-accent-foreground"
        @click="router.push({ name: 'runs' })"
      >
        <span
          class="h-2 w-2 rounded-full"
          :class="
            serverMeta.running_concurrency > 0 || serverMeta.running_qps > 0
              ? 'bg-emerald-500 animate-pulse'
              : 'bg-muted-foreground/45'
          "
        />
        <span class="font-medium text-foreground">
          {{ serverMeta.running_qps }} QPS
        </span>
        <span class="text-border">·</span>
        <span class="text-muted-foreground">
          {{ serverMeta.running_concurrency }} 并发
        </span>
      </button>

      <UiButton
        variant="outline"
        size="sm"
        :title="
          effectiveTheme === 'black'
            ? '当前：纯黑模式 (Black)，点击切换为浅色模式 (Light)'
            : '当前：浅色模式 (Light)，点击切换为纯黑模式 (Black)'
        "
        aria-label="切换明暗主题"
        @click="toggleTheme"
      >
        <Sun v-if="effectiveTheme === 'light'" class="h-3.5 w-3.5" />
        <Moon v-else class="h-3.5 w-3.5" />
        <span class="hidden md:inline font-mono text-xs">
          {{ effectiveTheme === "black" ? "Black" : "Light" }}
        </span>
      </UiButton>

      <UiButton
        v-if="serverMeta?.auth_required"
        variant="ghost"
        size="icon-sm"
        title="安全退出登录"
        aria-label="退出登录"
        @click="handleLogout"
      >
        <LogOut class="h-4 w-4" />
      </UiButton>
    </div>
  </header>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { LogOut, Moon, Sun } from "lucide-vue-next";
import { api } from "../../api/workspace";
import { effectiveTheme, toggleTheme } from "../../modules/theme";
import { toast } from "../../modules/toast";
import { serverMeta } from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";

const route = useRoute();
const router = useRouter();

const sectionTitle = computed(() => {
  switch (route.name) {
    case "workspace":
      return "接口调试与压测工作台";
    case "runs":
      return "压测任务中心";
    case "run-detail":
      return "压测实时监控与报告";
    case "profiles":
      return "可复用负载预设";
    case "docs":
      return "OpenAPI 3.1 接口规范";
    case "settings":
      return "系统外观与安全设置";
    default:
      return "Studio";
  }
});

async function handleLogout(): Promise<void> {
  await api.logout();
  toast.info("已安全退出");
  await router.push({ name: "login" });
}
</script>
