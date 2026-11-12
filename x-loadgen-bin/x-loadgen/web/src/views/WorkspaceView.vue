<template>
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
    <!-- Outer Splitpanes aligns top h-10 bar and right sidebar along a single vertical line -->
    <Splitpanes class="geist-splitter min-h-0 flex-1 overflow-hidden">
      <!-- Left: Multi-Tab Strip + Center Request / Collapsible Response Split -->
      <Pane
        :size="rightSidebarOpen ? 77 : 100"
        :min-size="50"
        class="flex min-h-0 flex-col overflow-hidden bg-background"
      >
        <div
          class="flex h-10 shrink-0 items-center justify-between border-b border-border bg-surface/60 select-none"
        >
          <div class="flex h-full min-w-0 flex-1 items-center overflow-x-auto">
            <div
              v-for="tab in tabs"
              :key="tab.tabId"
              class="group relative flex h-full min-w-[140px] max-w-[230px] cursor-pointer items-center gap-2 border-r border-border px-3.5 text-xs transition-colors"
              :class="
                activeTabId === tab.tabId
                  ? 'bg-background text-foreground font-medium'
                  : 'text-muted-foreground hover:bg-background/60 hover:text-foreground'
              "
              @click="activeTabId = tab.tabId"
              @dblclick="duplicateWorkspaceTab(tab.tabId)"
            >
              <span
                v-if="activeTabId === tab.tabId"
                class="absolute inset-x-0 top-0 h-[2px] bg-foreground"
              />
              <span
                class="shrink-0 font-mono text-[11px] font-semibold"
                :class="methodColorClass(tab.draft.method)"
              >
                {{ tab.draft.method }}
              </span>
              <span
                v-if="tab.draft.protocol === 'mazbri'"
                class="shrink-0 rounded border border-border bg-muted px-1 py-0.5 font-mono text-[10px] font-medium text-foreground"
              >
                MZ
              </span>
              <span class="flex-1 truncate">
                {{ tab.draft.name || "未命名请求" }}
              </span>
              <span
                v-if="isTabDirty(tab)"
                class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 group-hover:hidden"
              />
              <button
                type="button"
                title="关闭标签页"
                aria-label="关闭标签页"
                class="shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground opacity-60 transition-opacity hover:bg-accent hover:text-foreground group-hover:opacity-100"
                @click.stop="closeWorkspaceTab(tab.tabId)"
              >
                <X class="h-3 w-3" />
              </button>
            </div>

            <div class="flex shrink-0 items-center px-1.5">
              <UiButton
                variant="ghost"
                size="icon-xs"
                title="新建请求"
                aria-label="新建请求"
                @click="addNewRequestTab()"
              >
                <Plus class="h-3.5 w-3.5" />
              </UiButton>
            </div>
          </div>

          <EnvironmentSelector v-if="!rightSidebarOpen" />
        </div>

        <Splitpanes
          :horizontal="!horizontalSplit"
          class="geist-splitter min-h-0 flex-1 overflow-hidden"
        >
          <Pane
            :size="responsePaneOpen ? 54 : 100"
            :min-size="26"
            class="flex min-h-0 flex-col overflow-hidden bg-background"
          >
            <ProtocolAndBreadcrumbBar />
            <RequestUrlBar />
            <RequestOptionsTabs />
          </Pane>

          <Pane
            v-if="responsePaneOpen"
            :size="46"
            :min-size="22"
            class="flex min-h-0 flex-col overflow-hidden bg-background"
          >
            <ResponsePane />
          </Pane>
        </Splitpanes>
      </Pane>

      <!-- Collapsible Right Sidebar: Environment Bar + Collections / Environments / Profiles -->
      <Pane
        v-if="rightSidebarOpen"
        :size="23"
        :min-size="18"
        :max-size="40"
        class="flex min-h-0 flex-col overflow-hidden bg-background"
      >
        <div
          class="flex h-10 shrink-0 items-center border-b border-border bg-surface/60 select-none"
        >
          <EnvironmentSelector full-width />
        </div>
        <WorkspaceRightSidebar class="min-h-0 flex-1" />
      </Pane>
    </Splitpanes>
  </div>
</template>

<script setup lang="ts">
import { Pane, Splitpanes } from "splitpanes";
import { Plus, X } from "lucide-vue-next";
import UiButton from "../components/ui/UiButton.vue";
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
  responsePaneOpen,
  rightSidebarOpen,
  tabs,
} from "../stores/workspace";
</script>
