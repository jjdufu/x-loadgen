<template>
  <aside
    class="flex flex-col justify-between h-full bg-primary border-r border-dividerLight select-none flex-shrink-0 transition-all"
    :class="leftSidebarExpanded ? 'w-44' : 'w-14'"
  >
    <nav class="flex flex-col divide-y divide-dividerLight">
      <RouterLink
        v-for="item in navItems"
        :key="item.name"
        v-tippy="leftSidebarExpanded ? '' : item.label"
        :to="{ name: item.name }"
        class="nav-item flex items-center py-3.5 transition relative"
        :class="[
          leftSidebarExpanded ? 'px-4 space-x-3 justify-start' : 'px-2 justify-center',
          isActive(item.name)
            ? 'bg-primaryDark text-secondaryDark font-semibold active-rail'
            : 'text-secondaryLight hover:text-secondaryDark hover:bg-primaryLight/60',
        ]"
      >
        <component :is="item.icon" class="svg-icons" />
        <span v-if="leftSidebarExpanded" class="truncate text-body">
          {{ item.shortLabel }}
        </span>
      </RouterLink>
    </nav>

    <div class="flex flex-col divide-y divide-dividerLight border-t border-dividerLight">
      <RouterLink
        v-tippy="leftSidebarExpanded ? '' : '外观主题与安全设置'"
        :to="{ name: 'settings' }"
        class="nav-item flex items-center py-3.5 transition relative"
        :class="[
          leftSidebarExpanded ? 'px-4 space-x-3 justify-start' : 'px-2 justify-center',
          isActive('settings')
            ? 'bg-primaryDark text-secondaryDark font-semibold active-rail'
            : 'text-secondaryLight hover:text-secondaryDark hover:bg-primaryLight/60',
        ]"
      >
        <Settings class="svg-icons" />
        <span v-if="leftSidebarExpanded" class="truncate text-body">设置</span>
      </RouterLink>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { useRoute } from "vue-router";
import { Activity, Gauge, Globe, Settings } from "lucide-vue-next";
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
    label: "压测任务中心与实时监控 (Runs)",
    shortLabel: "压测任务",
    icon: Activity,
  },
  {
    name: "profiles",
    label: "可复用压测配置模板 (Load Profiles)",
    shortLabel: "压测模板",
    icon: Gauge,
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

<style scoped>
.active-rail::before {
  content: "";
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 2px;
  background-color: var(--accent-color);
}
</style>
