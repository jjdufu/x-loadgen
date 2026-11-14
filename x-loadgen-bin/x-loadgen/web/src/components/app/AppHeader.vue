<template>
  <header
    class="z-20 flex h-12 shrink-0 items-center justify-between border-b border-border bg-background px-4 select-none"
  >
    <!-- Minimalist Mark & Unified Breadcrumb -->
    <div class="flex min-w-0 items-center gap-2.5">
      <UiButton
        v-if="isMobile"
        variant="ghost"
        size="icon-sm"
        class="-ml-1.5"
        aria-label="打开导航菜单"
        aria-controls="app-mobile-nav"
        :aria-expanded="mobileNavOpen"
        @click="mobileNavOpen = !mobileNavOpen"
      >
        <Menu class="h-4 w-4" />
      </UiButton>
      <RouterLink
        :to="{ name: 'workspace' }"
        title="返回工作台"
        aria-label="XL 返回工作台"
        class="inline-flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md bg-foreground font-mono text-[11px] font-bold tracking-tighter text-background transition-opacity hover:opacity-80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      >
        XL
      </RouterLink>

      <span class="cursor-default text-border select-none">/</span>

      <template v-if="route.name === 'run-detail'">
        <span class="cursor-default truncate text-sm font-medium text-muted-foreground select-none">
          压测任务
        </span>
        <span class="cursor-default text-border select-none">/</span>
        <span class="cursor-default truncate text-sm font-medium text-foreground select-none">
          任务详情
        </span>
      </template>
      <span v-else class="cursor-default truncate text-sm font-medium text-foreground select-none">
        {{ sectionTitle }}
      </span>
    </div>

    <!-- Top-Right Icon-Only Controls -->
    <div class="flex items-center gap-1 shrink-0">
      <UiButton
        variant="ghost"
        size="icon-sm"
        :title="effectiveTheme === 'black' ? '切换为浅色模式' : '切换为纯黑模式'"
        aria-label="切换外观模式"
        @click="toggleTheme"
      >
        <Sun v-if="effectiveTheme === 'light'" class="h-4 w-4" />
        <Moon v-else class="h-4 w-4" />
      </UiButton>

      <UiButton
        variant="ghost"
        size="icon-sm"
        :class="
          route.name === 'settings'
            ? 'bg-accent text-accent-foreground'
            : ''
        "
        title="系统设置"
        aria-label="系统设置"
        @click="router.push({ name: 'settings' })"
      >
        <Settings class="h-4 w-4" />
      </UiButton>

      <UiButton
        v-if="serverMeta?.auth_required"
        variant="ghost"
        size="icon-sm"
        title="退出登录"
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
import { LogOut, Menu, Moon, Settings, Sun } from "lucide-vue-next";
import { api } from "../../api/workspace";
import { invalidateCachedMe } from "../../modules/auth";
import { effectiveTheme, toggleTheme } from "../../modules/theme";
import { isMobile, mobileNavOpen } from "../../modules/viewport";
import { toast } from "../../modules/toast";
import { clearWorkspaceSession, serverMeta } from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";

const route = useRoute();
const router = useRouter();

const sectionTitle = computed(() => {
  if (typeof route.meta.title === "string") {
    return route.meta.title;
  }
  return "工作台";
});

async function handleLogout(): Promise<void> {
  await api.logout();
  invalidateCachedMe();
  clearWorkspaceSession();
  toast.info("已退出登录");
  await router.push({ name: "login" });
}
</script>
