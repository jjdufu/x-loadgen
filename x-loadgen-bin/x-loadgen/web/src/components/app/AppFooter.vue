<template>
  <footer
    class="flex h-8 shrink-0 items-center justify-between border-t border-border bg-background px-3 text-xs text-muted-foreground select-none"
  >
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        :title="leftSidebarExpanded ? '收起左侧导航栏' : '展开左侧导航栏'"
        class="inline-flex h-6 w-6 items-center justify-center rounded hover:bg-accent hover:text-foreground transition-colors"
        @click="leftSidebarExpanded = !leftSidebarExpanded"
      >
        <PanelLeft class="h-3.5 w-3.5" />
      </button>
      <span class="hidden sm:inline-flex items-center gap-1.5 px-1.5 font-mono text-[11px] text-muted-foreground">
        <span>Dual-Protocol Engine:</span>
        <span class="text-foreground">HTTP</span>
        <span>·</span>
        <span class="text-foreground">Mazbri Envelope &amp; Signature</span>
      </span>
    </div>

    <div class="flex items-center gap-1">
      <button
        v-if="isWorkspaceRoute"
        type="button"
        :title="horizontalSplit ? '切换为上下分栏' : '切换为左右分栏'"
        class="inline-flex h-6 w-6 items-center justify-center rounded hover:bg-accent hover:text-foreground transition-colors"
        @click="horizontalSplit = !horizontalSplit"
      >
        <Columns2
          class="h-3.5 w-3.5 transition-transform"
          :class="{ 'rotate-90': !horizontalSplit }"
        />
      </button>

      <button
        v-if="isWorkspaceRoute"
        type="button"
        :title="rightSidebarOpen ? '收起右侧集合与环境栏' : '展开右侧集合与环境栏'"
        class="inline-flex h-6 w-6 items-center justify-center rounded hover:bg-accent hover:text-foreground transition-colors"
        @click="rightSidebarOpen = !rightSidebarOpen"
      >
        <PanelRight class="h-3.5 w-3.5" />
      </button>

      <button
        type="button"
        title="查看键盘快捷键 (Ctrl + /)"
        class="inline-flex h-6 items-center gap-1.5 rounded px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        @click="shortcutsModalOpen = true"
      >
        <Keyboard class="h-3.5 w-3.5" />
        <span>快捷键</span>
        <kbd
          class="hidden sm:inline-block rounded border border-border bg-muted px-1 py-0.2 font-mono text-[10px] text-muted-foreground"
        >
          Ctrl /
        </kbd>
      </button>
    </div>
  </footer>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import { Columns2, Keyboard, PanelLeft, PanelRight } from "lucide-vue-next";
import {
  horizontalSplit,
  leftSidebarExpanded,
  rightSidebarOpen,
  shortcutsModalOpen,
} from "../../stores/workspace";

const route = useRoute();
const isWorkspaceRoute = computed(() => route.name === "workspace");
</script>
