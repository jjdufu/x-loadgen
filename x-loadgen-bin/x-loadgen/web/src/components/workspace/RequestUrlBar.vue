<template>
  <div
    class="flex flex-wrap items-center gap-2 border-b border-border bg-background px-4 py-2.5"
  >
    <!-- Shadcn Monochrome Segmented Protocol Switcher -->
    <div
      class="inline-flex h-9 shrink-0 items-center rounded-lg border border-border bg-muted p-0.5"
      role="group"
      aria-label="选择请求协议"
    >
      <button
        type="button"
        class="inline-flex h-full items-center gap-1.5 rounded-md px-2.5 text-xs font-medium whitespace-nowrap transition-all"
        :class="
          draft.protocol === 'http'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'
        "
        @click="switchActiveTabProtocol('http')"
      >
        <Globe class="h-3.5 w-3.5" />
        <span>HTTP 通用</span>
      </button>
      <button
        type="button"
        class="inline-flex h-full items-center gap-1.5 rounded-md px-2.5 text-xs font-medium whitespace-nowrap transition-all"
        :class="
          draft.protocol === 'mazbri'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'
        "
        @click="switchActiveTabProtocol('mazbri')"
      >
        <ShieldCheck class="h-3.5 w-3.5" />
        <span>Mazbri 网关</span>
      </button>
    </div>

    <!-- Unified Method + Geist Mono URL Input -->
    <div
      class="flex h-9 min-w-[220px] flex-1 items-center overflow-hidden rounded-md border border-input bg-background shadow-sm transition-colors focus-within:ring-1 focus-within:ring-ring"
    >
      <div class="relative flex h-full items-center border-r border-border bg-muted/35">
        <select
          :value="draft.method"
          aria-label="HTTP 方法"
          class="h-full cursor-pointer appearance-none bg-transparent pl-3 pr-7 font-mono text-xs font-semibold focus:outline-none"
          :class="methodColorClass(draft.method)"
          @change="draft.method = ($event.target as HTMLSelectElement).value"
        >
          <option
            v-for="m in HTTP_METHODS"
            :key="m"
            :value="m"
          >
            {{ m }}
          </option>
        </select>
        <ChevronDown
          class="pointer-events-none absolute right-2 h-3 w-3 text-muted-foreground"
        />
      </div>

      <span
        v-if="draft.protocol === 'mazbri'"
        title="Mazbri 模式：填写网关内部业务路由路径，外层自动经 /api/v1/gateway 密封转发"
        class="ml-2 shrink-0 rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground"
      >
        inner.path
      </span>

      <input
        :value="draft.endpoint"
        type="text"
        aria-label="请求地址或路径"
        :placeholder="
          draft.protocol === 'mazbri'
            ? '输入网关业务路径，如 /v1/orders/<<orderId>>'
            : '输入完整 URL 或环境相对路径，支持 <<var>> 与 {{seq}}'
        "
        class="h-full min-w-0 flex-1 bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
        @input="draft.endpoint = ($event.target as HTMLInputElement).value"
        @keydown.enter.exact.prevent="sendActiveTabProbe"
      />
    </div>

    <!-- Action Buttons: wraps cleanly in narrow horizontal split without clipping -->
    <div class="ml-auto flex shrink-0 items-center gap-1.5">
      <UiButton
        variant="default"
        class="px-3"
        :loading="activeTab.sending"
        @click="sendActiveTabProbe"
      >
        <Send v-if="!activeTab.sending" class="h-3.5 w-3.5" />
        <span>发送</span>
      </UiButton>

      <UiButton
        variant="outline"
        class="px-3"
        title="发起压测"
        @click="openSingleLoadRunner(draft)"
      >
        <Zap class="h-3.5 w-3.5" />
        <span>压测</span>
      </UiButton>

      <UiButton
        variant="outline"
        class="px-3"
        title="保存请求"
        @click="saveActiveTab"
      >
        <Save class="h-3.5 w-3.5" />
        <span>保存</span>
      </UiButton>

      <UiDropdown align="end">
        <template #trigger>
          <UiButton
            variant="ghost"
            size="icon"
            title="更多操作"
            aria-label="更多操作"
          >
            <MoreHorizontal class="h-4 w-4" />
          </UiButton>
        </template>

        <UiDropdownItem @select="curlModalOpen = true">
          <Terminal class="h-4 w-4 text-muted-foreground" />
          <span>从 cURL 导入...</span>
        </UiDropdownItem>
        <UiDropdownItem @select="openSaveAs">
          <CopyPlus class="h-4 w-4 text-muted-foreground" />
          <span>另存为...</span>
        </UiDropdownItem>
        <UiDropdownItem @select="duplicateWorkspaceTab(activeTab.tabId)">
          <Copy class="h-4 w-4 text-muted-foreground" />
          <span>复制标签页</span>
        </UiDropdownItem>
      </UiDropdown>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  ChevronDown,
  Copy,
  CopyPlus,
  Globe,
  MoreHorizontal,
  Save,
  Send,
  ShieldCheck,
  Terminal,
  Zap,
} from "lucide-vue-next";
import { HTTP_METHODS, methodColorClass } from "../../helpers/format";
import {
  activeTab,
  curlModalOpen,
  duplicateWorkspaceTab,
  openSingleLoadRunner,
  saveActiveTab,
  saveModalForceNew,
  saveModalOpen,
  sendActiveTabProbe,
  switchActiveTabProtocol,
} from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiDropdown from "../ui/UiDropdown.vue";
import UiDropdownItem from "../ui/UiDropdownItem.vue";

const draft = computed(() => activeTab.value.draft);

function openSaveAs(): void {
  saveModalForceNew.value = true;
  saveModalOpen.value = true;
}
</script>
