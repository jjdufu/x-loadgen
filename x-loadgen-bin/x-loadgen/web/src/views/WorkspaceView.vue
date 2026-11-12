<template>
  <div class="flex flex-col flex-1 min-h-0 min-w-0 overflow-hidden">
    <!-- Top Multi-Tab Strip + Environment Selector (Hoppscotch Tab Header) -->
    <div class="flex items-center justify-between border-b border-dividerLight bg-primaryDark select-none">
      <div class="flex items-center overflow-x-auto no-scrollbar flex-1 min-w-0">
        <div
          v-for="tab in tabs"
          :key="tab.tabId"
          class="flex items-center space-x-2 px-3 py-2 border-r border-dividerLight cursor-pointer transition min-w-[140px] max-w-[230px] group relative"
          :class="
            activeTabId === tab.tabId
              ? 'bg-primary text-secondaryDark font-semibold active-top-tab'
              : 'bg-primaryDark text-secondary hover:bg-primaryLight/60'
          "
          @click="activeTabId = tab.tabId"
          @dblclick="duplicateWorkspaceTab(tab.tabId)"
        >
          <span
            class="font-mono font-bold text-[10px] flex-shrink-0"
            :class="methodColorClass(tab.draft.method)"
          >
            {{ tab.draft.method }}
          </span>
          <span
            v-if="tab.draft.protocol === 'mazbri'"
            class="px-1 rounded bg-accent/15 text-accent font-mono text-[9px] flex-shrink-0"
          >
            MZ
          </span>
          <span class="truncate text-tiny flex-1">
            {{ tab.draft.name || "未命名请求" }}
          </span>
          <span
            v-if="isTabDirty(tab)"
            class="w-1.5 h-1.5 rounded-full bg-amber-500 flex-shrink-0 group-hover:hidden"
          />
          <button
            v-tippy="'关闭标签页'"
            type="button"
            class="p-0.5 rounded hover:bg-divider text-secondaryLight hover:text-secondaryDark opacity-60 group-hover:opacity-100 flex-shrink-0"
            @click.stop="closeWorkspaceTab(tab.tabId)"
          >
            <X class="w-3 h-3" />
          </button>
        </div>

        <div class="flex items-center px-1.5 space-x-0.5 flex-shrink-0">
          <HoppButtonSecondary
            v-tippy="'新建 HTTP 请求标签页 (Alt + N)'"
            :icon="Plus"
            @click="addNewRequestTab('http')"
          />
          <HoppButtonSecondary
            v-tippy="'新建 Mazbri 加密网关请求标签页 (Alt + M)'"
            :icon="ShieldPlus"
            @click="addNewRequestTab('mazbri')"
          />
        </div>
      </div>

      <EnvironmentSelector />
    </div>

    <!-- Main Hoppscotch Splitpanes Area -->
    <Splitpanes class="smart-splitter flex-1 min-h-0 overflow-hidden">
      <!-- Center Request + Response Split -->
      <Pane :size="rightSidebarOpen ? 72 : 100" :min-size="45" class="flex flex-col min-h-0 overflow-hidden bg-primary">
        <Splitpanes
          :horizontal="!horizontalSplit"
          class="smart-splitter flex-1 min-h-0 overflow-hidden"
        >
          <Pane :size="52" :min-size="25" class="flex flex-col min-h-0 overflow-hidden bg-primary">
            <ProtocolAndBreadcrumbBar />
            <RequestUrlBar />
            <RequestOptionsTabs />
          </Pane>

          <Pane :size="48" :min-size="22" class="flex flex-col min-h-0 overflow-hidden bg-primary border-t border-dividerLight">
            <ResponsePane />
          </Pane>
        </Splitpanes>
      </Pane>

      <!-- Collapsible Right Sidebar (Collections / Environments / Profiles) -->
      <Pane
        v-if="rightSidebarOpen"
        :size="28"
        :min-size="18"
        :max-size="45"
        class="flex flex-col min-h-0 overflow-hidden bg-primary"
      >
        <WorkspaceRightSidebar />
      </Pane>
    </Splitpanes>
  </div>
</template>

<script setup lang="ts">
import { HoppButtonSecondary } from "@hoppscotch/ui";
import { Pane, Splitpanes } from "splitpanes";
import "splitpanes/dist/splitpanes.css";
import { Plus, ShieldPlus, X } from "lucide-vue-next";
import EnvironmentSelector from "../components/workspace/EnvironmentSelector.vue";
import ProtocolAndBreadcrumbBar from "../components/workspace/ProtocolAndBreadcrumbBar.vue";
import RequestOptionsTabs from "../components/workspace/RequestOptionsTabs.vue";
import RequestUrlBar from "../components/workspace/RequestUrlBar.vue";
import ResponsePane from "../components/workspace/ResponsePane.vue";
import WorkspaceRightSidebar from "../components/workspace/WorkspaceRightSidebar.vue";
import { methodColorClass } from "../helpers/format";
import {
  activeTabId,
  addNewRequestTab,
  closeWorkspaceTab,
  duplicateWorkspaceTab,
  horizontalSplit,
  isTabDirty,
  rightSidebarOpen,
  tabs,
} from "../stores/workspace";
</script>

<style scoped>
.active-top-tab::before {
  content: "";
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 2px;
  background-color: var(--accent-color);
}
</style>
