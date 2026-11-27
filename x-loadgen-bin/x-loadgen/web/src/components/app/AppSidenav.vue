<template>
  <!-- Phone: navigation lives in a drawer opened from the header. -->
  <template v-if="isMobile">
    <Transition name="fade">
      <div
        v-if="mobileNavOpen"
        class="fixed inset-0 z-40 bg-black/40"
        aria-hidden="true"
        @click="mobileNavOpen = false"
      />
    </Transition>
    <aside
      id="app-mobile-nav"
      class="fixed inset-y-0 left-0 z-50 flex w-64 max-w-[80vw] flex-col border-r border-border bg-background transition-[transform,box-shadow] duration-200 select-none"
      :class="mobileNavOpen ? 'translate-x-0 shadow-xl' : '-translate-x-full shadow-none'"
      :aria-hidden="!mobileNavOpen"
      :inert="!mobileNavOpen"
    >
      <div class="flex h-12 items-center border-b border-border px-4 text-sm font-semibold">
        导航
      </div>
      <nav class="flex flex-col gap-1 p-2" aria-label="主导航">
        <RouterLink
          v-for="item in navItems"
          :key="item.name"
          :to="{ name: item.name }"
          class="flex h-11 items-center gap-3 rounded-md px-3 text-sm transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
          :class="
            isActive(item.name)
              ? 'bg-accent text-accent-foreground font-medium hover:bg-accent-hover active:bg-active-bg'
              : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'
          "
          :aria-current="isActive(item.name) ? 'page' : undefined"
        >
          <component :is="item.icon" class="h-4 w-4 shrink-0" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </nav>
    </aside>
  </template>

  <aside
    v-else
    class="flex h-full shrink-0 flex-col border-r border-border bg-surface/60 select-none transition-[width] duration-150"
    :class="expanded ? 'w-44' : 'w-14'"
  >
    <nav class="flex flex-col gap-1 p-2" aria-label="主导航">
      <RouterLink
        v-for="item in navItems"
        :key="item.name"
        :to="{ name: item.name }"
        :title="expanded ? undefined : item.label"
        :aria-label="item.label"
        :aria-current="isActive(item.name) ? 'page' : undefined"
        class="flex items-center rounded-md text-sm transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
        :class="[
          expanded
            ? 'h-9 gap-2.5 px-3 justify-start'
            : 'h-9 w-10 mx-auto justify-center',
          isActive(item.name)
            ? 'bg-accent text-accent-foreground font-medium shadow-sm hover:bg-accent-hover active:bg-active-bg'
            : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg',
        ]"
      >
        <component :is="item.icon" class="h-4 w-4 shrink-0" />
        <span v-if="expanded" class="truncate whitespace-nowrap">
          {{ item.label }}
        </span>
      </RouterLink>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { computed, watch } from "vue";
import { useRoute } from "vue-router";
import { Activity, BookOpen, Gauge, Globe, History, Settings } from "lucide-vue-next";
import { isCompact, isMobile, mobileNavOpen } from "../../modules/viewport";
import { leftSidebarExpanded } from "../../stores/workspace";

const route = useRoute();

/** Tablets always get the icon rail; the stored preference applies on desktop. */
const expanded = computed(() => leftSidebarExpanded.value && !isCompact.value);

watch(
  () => route.fullPath,
  () => {
    mobileNavOpen.value = false;
  },
);
watch(isMobile, (mobile) => {
  if (!mobile) mobileNavOpen.value = false;
});

const navItems = [
  {
    name: "workspace",
    label: "工作台",
    icon: Globe,
  },
  {
    name: "history",
    label: "历史记录",
    icon: History,
  },
  {
    name: "runs",
    label: "压测报告",
    icon: Activity,
  },
  {
    name: "profiles",
    label: "负载预设",
    icon: Gauge,
  },
  {
    name: "docs",
    label: "接口文档",
    icon: BookOpen,
  },
  {
    name: "settings",
    label: "系统设置",
    icon: Settings,
  },
] as const;

function isActive(targetName: string): boolean {
  if (targetName === "runs") {
    return (
      route.name === "runs" ||
      route.name === "run-detail" ||
      route.name === "run-report" ||
      route.name === "run-compare"
    );
  }
  return route.name === targetName;
}
</script>
