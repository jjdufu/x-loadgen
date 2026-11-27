<template>
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
    <!-- Outer Splitpanes aligns top h-10 bar and right sidebar along a single vertical line -->
    <Splitpanes class="geist-splitter min-h-0 flex-1 overflow-hidden">
      <!-- Left: Multi-Tab Strip + Center Request / Collapsible Response Split -->
      <Pane
        :size="rightDocked ? 77 : 100"
        :min-size="50"
        class="flex min-h-0 flex-col overflow-hidden bg-background"
      >
        <div
          class="flex h-10 shrink-0 items-center justify-between border-b border-border bg-surface/60 select-none"
        >
          <div
            role="tablist"
            aria-label="请求标签页"
            class="flex h-full min-w-0 flex-1 items-center overflow-x-auto"
          >
            <div
              v-for="(tab, idx) in tabs"
              :key="tab.tabId"
              role="tab"
              :tabindex="activeTabId === tab.tabId ? 0 : -1"
              :aria-selected="activeTabId === tab.tabId"
              class="group relative flex h-full min-w-[140px] max-w-[230px] cursor-pointer items-center gap-2 border-r border-border px-3 text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
              :class="
                activeTabId === tab.tabId
                  ? 'bg-background text-foreground font-medium'
                  : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground'
              "
              @click="activeTabId = tab.tabId"
              @dblclick="duplicateWorkspaceTab(tab.tabId)"
              @keydown.enter.self="activeTabId = tab.tabId"
              @keydown.space.prevent.self="activeTabId = tab.tabId"
              @keydown.arrow-right.prevent="focusNeighborTab(idx, 1, $event)"
              @keydown.arrow-left.prevent="focusNeighborTab(idx, -1, $event)"
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
                class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 group-hover:hidden group-focus-within:hidden"
              />
              <button
                type="button"
                title="关闭标签页"
                aria-label="关闭标签页"
                class="inline-flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground opacity-60 transition-colors duration-100 hover:bg-hover-bg hover:text-foreground active:bg-active-bg group-hover:opacity-100 group-focus-within:opacity-100"
                @click.stop="closeWorkspaceTab(tab.tabId)"
              >
                <X class="h-3.5 w-3.5" />
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

          <EnvironmentSelector v-if="!rightDocked && !isMobile" />
          <UiButton
            v-if="isCompact"
            variant="ghost"
            size="icon-sm"
            class="mx-1 shrink-0"
            title="集合 / 环境 / 预设 / 历史"
            aria-label="侧边面板"
            aria-controls="workspace-right-drawer"
            :aria-expanded="compactRightPanelOpen"
            @click="compactRightPanelOpen = true"
          >
            <PanelRight class="h-4 w-4" />
          </UiButton>
        </div>

        <!-- Phones always stack request above response; side-by-side has no room. -->
        <Splitpanes
          :horizontal="isMobile || !horizontalSplit"
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
        v-if="rightDocked"
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

    <!-- Compact viewports: the right panel slides over the editor instead of squeezing it. -->
    <template v-if="isCompact">
      <Transition name="fade">
        <div
          v-if="compactRightPanelOpen"
          class="fixed inset-0 z-40 bg-black/40"
          aria-hidden="true"
          @click="compactRightPanelOpen = false"
        />
      </Transition>
      <aside
        id="workspace-right-drawer"
        class="fixed inset-y-0 right-0 z-50 flex w-[360px] max-w-[88vw] flex-col border-l border-border bg-background transition-[transform,box-shadow] duration-200"
        :class="compactRightPanelOpen ? 'translate-x-0 shadow-xl' : 'translate-x-full shadow-none'"
        :aria-hidden="!compactRightPanelOpen"
        :inert="!compactRightPanelOpen"
        aria-label="集合、环境与预设"
      >
        <div
          class="flex h-12 shrink-0 items-center gap-1 border-b border-border bg-surface/60 pr-1"
        >
          <EnvironmentSelector full-width />
          <UiButton
            variant="ghost"
            size="icon-sm"
            class="shrink-0"
            aria-label="关闭面板"
            @click="compactRightPanelOpen = false"
          >
            <X class="h-4 w-4" />
          </UiButton>
        </div>
        <WorkspaceRightSidebar class="min-h-0 flex-1" />
      </aside>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, watch } from "vue";
import { Pane, Splitpanes } from "splitpanes";
import { PanelRight, Plus, X } from "lucide-vue-next";
import UiButton from "../components/ui/UiButton.vue";
import EnvironmentSelector from "../components/workspace/EnvironmentSelector.vue";
import ProtocolAndBreadcrumbBar from "../components/workspace/ProtocolAndBreadcrumbBar.vue";
import RequestOptionsTabs from "../components/workspace/RequestOptionsTabs.vue";
import RequestUrlBar from "../components/workspace/RequestUrlBar.vue";
import ResponsePane from "../components/workspace/ResponsePane.vue";
import WorkspaceRightSidebar from "../components/workspace/WorkspaceRightSidebar.vue";
import { methodColorClass } from "../helpers/format";
import { canProgrammaticallyFocus } from "../modules/activity";
import {
  compactRightPanelOpen,
  isCompact,
  isMobile,
} from "../modules/viewport";
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

/** The right panel is docked as a split pane only on wide viewports. */
const rightDocked = computed(() => rightSidebarOpen.value && !isCompact.value);

watch(isCompact, (compact) => {
  if (!compact) compactRightPanelOpen.value = false;
});
// Opening a saved request from the drawer should reveal the editor.
watch(activeTabId, () => {
  if (isCompact.value) compactRightPanelOpen.value = false;
});

function focusNeighborTab(idx: number, delta: number, event: KeyboardEvent): void {
  const count = tabs.value.length;
  if (count === 0) return;
  const nextIdx = (idx + delta + count) % count;
  const nextTab = tabs.value[nextIdx];
  if (!nextTab) return;
  activeTabId.value = nextTab.tabId;
  const tablist = (event.currentTarget as HTMLElement | null)?.parentElement;
  const tabNodes = tablist?.querySelectorAll<HTMLElement>('[role="tab"]');
  if (canProgrammaticallyFocus()) {
    tabNodes?.[nextIdx]?.focus();
  }
}
</script>
