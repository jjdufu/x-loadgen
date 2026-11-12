<template>
  <div class="flex flex-wrap sm:flex-nowrap items-stretch gap-2 px-4 py-2.5 border-b border-dividerLight bg-primary">
    <!-- Combined Method + URL Input Group (Hoppscotch style) -->
    <div
      class="flex flex-1 min-w-0 items-center rounded border border-divider bg-primaryLight focus-within:border-accent transition overflow-hidden"
    >
      <select
        :value="draft.method"
        aria-label="HTTP 方法"
        class="px-3 py-2 bg-transparent font-mono font-bold text-tiny cursor-pointer border-r border-dividerLight"
        :class="methodColorClass(draft.method)"
        @change="draft.method = ($event.target as HTMLSelectElement).value"
      >
        <option v-for="m in HTTP_METHODS" :key="m" :value="m" class="bg-primary text-secondaryDark">
          {{ m }}
        </option>
      </select>

      <span
        v-if="draft.protocol === 'mazbri'"
        v-tippy="'Mazbri 模式：此处填写网关内部业务路由 path，外层统一经 /api/v1/gateway 密封转发'"
        class="px-2 py-0.5 ml-2 rounded bg-accent/15 text-accent font-mono text-[11px] flex-shrink-0"
      >
        inner.path
      </span>

      <input
        :value="draft.endpoint"
        type="text"
        :placeholder="
          draft.protocol === 'mazbri'
            ? '输入业务接口路径，如 /v1/orders/<<orderId>>'
            : '输入完整 URL (https://...) 或相对环境网关路径 (/api/v1/...)，支持 <<var>> 与 {{seq}}'
        "
        class="flex-1 min-w-0 px-3 py-2 bg-transparent font-mono text-secondaryDark placeholder-secondaryLight"
        @input="draft.endpoint = ($event.target as HTMLInputElement).value"
        @keydown.enter.exact.prevent="sendActiveTabProbe"
      />

      <HoppButtonSecondary
        v-if="draft.protocol === 'http'"
        v-tippy="'从 cURL 命令导入'"
        :icon="Terminal"
        label="cURL"
        @click="curlModalOpen = true"
      />
    </div>

    <!-- Send / Load Test / Save Action Buttons -->
    <div class="flex items-center space-x-1.5 flex-shrink-0">
      <HoppButtonPrimary
        :icon="Send"
        :loading="activeTab.sending"
        label="发送 (Send)"
        @click="sendActiveTabProbe"
      />

      <HoppButtonSecondary
        v-tippy="'以此请求发起并发压测 (Load Test)'"
        :icon="Zap"
        label="压测"
        filled
        outline
        class="!text-accent !border-accent/60 hover:!bg-accent/15"
        @click="openSingleLoadRunner(draft)"
      />

      <HoppButtonSecondary
        v-tippy="'保存请求到集合 (Ctrl + S)'"
        :icon="Save"
        label="保存"
        outline
        @click="saveActiveTab"
      />

      <HoppButtonSecondary
        v-tippy="'另存为新请求'"
        :icon="CopyPlus"
        outline
        @click="openSaveAs"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { HoppButtonPrimary, HoppButtonSecondary } from "@hoppscotch/ui";
import { CopyPlus, Save, Send, Terminal, Zap } from "lucide-vue-next";
import { HTTP_METHODS, methodColorClass } from "../../helpers/format";
import {
  activeTab,
  curlModalOpen,
  openSingleLoadRunner,
  saveActiveTab,
  saveModalForceNew,
  saveModalOpen,
  sendActiveTabProbe,
} from "../../stores/workspace";

const draft = computed(() => activeTab.value.draft);

function openSaveAs(): void {
  saveModalForceNew.value = true;
  saveModalOpen.value = true;
}
</script>
