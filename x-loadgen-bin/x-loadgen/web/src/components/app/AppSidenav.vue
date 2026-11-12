<template>
  <aside
    class="flex h-full shrink-0 flex-col justify-between border-r border-border bg-surface/60 select-none transition-[width] duration-150"
    :class="leftSidebarExpanded ? 'w-48' : 'w-14'"
  >
    <nav class="flex flex-col gap-1 p-2">
      <RouterLink
        v-for="item in navItems"
        :key="item.name"
        :to="{ name: item.name }"
        :title="leftSidebarExpanded ? undefined : item.label"
        class="flex items-center rounded-md text-sm transition-colors"
        :class="[
          leftSidebarExpanded
            ? 'h-9 gap-2.5 px-3 justify-start'
            : 'h-9 w-10 mx-auto justify-center',
          isActive(item.name)
            ? 'bg-accent text-accent-foreground font-medium shadow-2xs'
            : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
        ]"
      >
        <component :is="item.icon" class="h-4 w-4 shrink-0" />
        <span v-if="leftSidebarExpanded" class="truncate whitespace-nowrap">
          {{ item.shortLabel }}
        </span>
      </RouterLink>
    </nav>

    <div class="border-t border-border p-2">
      <RouterLink
        :to="{ name: 'settings' }"
        :title="leftSidebarExpanded ? undefined : '外观与安全设置'"
        class="flex items-center rounded-md text-sm transition-colors"
        :class="[
          leftSidebarExpanded
            ? 'h-9 gap-2.5 px-3 justify-start'
            : 'h-9 w-10 mx-auto justify-center',
          isActive('settings')
            ? 'bg-accent text-accent-foreground font-medium'
            : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
        ]"
      >
        <Settings class="h-4 w-4 shrink-0" />
        <span v-if="leftSidebarExpanded" class="truncate whitespace-nowrap">
          系统设置
        </span>
      </RouterLink>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { useRoute } from "vue-router";
import { Activity, BookOpen, Gauge, Globe, Settings } from "lucide-vue-next";
import { leftSidebarExpanded } from "../../stores/workspace";

const route = useRoute();

const navItems = [
  {
    name: "workspace",
    label: "接口调试与压测工作台 (HTTP / Mazbri)",
    shortLabel: "接口工作台",
    icon: Globe,
  },
  {
    name: "runs",
    label: "压测任务中心与实时监控",
    shortLabel: "压测任务",
    icon: Activity,
  },
  {
    name: "profiles",
    label: "可复用负载配置预设",
    shortLabel: "负载预设",
    icon: Gauge,
  },
  {
    name: "docs",
    label: "内置 OpenAPI 3.1 接口规范文档",
    shortLabel: "接口文档",
    icon: BookOpen,
  },
] as const;

function isActive(targetName: string): boolean {
  if (targetName === "runs") {
    return (
      route.name === "runs" ||
      route.name === "run-detail" ||
      route.name === "run-report"
    );
  }
  return route.name === targetName;
}
</script>
