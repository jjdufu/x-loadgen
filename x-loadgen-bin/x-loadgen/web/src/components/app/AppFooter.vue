<template>
  <footer
    class="flex items-center justify-between px-2 h-8 border-t border-dividerLight bg-primary text-tiny text-secondaryLight select-none flex-shrink-0"
  >
    <div class="flex items-center space-x-1">
      <HoppButtonSecondary
        v-tippy="leftSidebarExpanded ? '收起导航文字' : '展开导航文字'"
        :icon="PanelLeft"
        @click="leftSidebarExpanded = !leftSidebarExpanded"
      />
      <span class="hidden sm:inline-flex items-center space-x-1.5 px-2 text-secondary">
        <Shield class="w-3.5 h-3.5 text-accent" />
        <span>Standalone Dual-Protocol: HTTP &amp; Mazbri Crypto</span>
      </span>
    </div>

    <div class="flex items-center space-x-1">
      <HoppButtonSecondary
        v-tippy="'键盘快捷键 (Ctrl + /)'"
        :icon="Zap"
        label="快捷键"
        @click="shortcutsModalOpen = true"
      />
      <HoppButtonSecondary
        v-if="isWorkspaceRoute"
        v-tippy="horizontalSplit ? '切换为左右双栏布局' : '切换为上下双栏布局'"
        :icon="Columns2"
        :class="{ 'transform rotate-90': !horizontalSplit }"
        @click="horizontalSplit = !horizontalSplit"
      />
      <HoppButtonSecondary
        v-if="isWorkspaceRoute"
        v-tippy="rightSidebarOpen ? '收起右侧集合/环境栏' : '展开右侧集合/环境栏'"
        :icon="PanelRight"
        @click="rightSidebarOpen = !rightSidebarOpen"
      />
    </div>
  </footer>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import { HoppButtonSecondary } from "@hoppscotch/ui";
import { Columns2, PanelLeft, PanelRight, Shield, Zap } from "lucide-vue-next";
import {
  horizontalSplit,
  leftSidebarExpanded,
  rightSidebarOpen,
  shortcutsModalOpen,
} from "../../stores/workspace";

const route = useRoute();
const isWorkspaceRoute = computed(() => route.name === "workspace");
</script>
