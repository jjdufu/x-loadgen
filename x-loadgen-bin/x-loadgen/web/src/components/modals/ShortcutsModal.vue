<template>
  <HoppSmartModal
    v-if="shortcutsModalOpen"
    dialog
    title="工作台快捷键与操作指南"
    @close="shortcutsModalOpen = false"
  >
    <template #body>
      <div class="divide-y divide-dividerLight border border-dividerLight rounded">
        <div
          v-for="item in shortcuts"
          :key="item.keys"
          class="flex items-center justify-between px-4 py-2.5"
        >
          <span class="text-secondaryDark">{{ item.desc }}</span>
          <span class="font-mono text-tiny px-2 py-0.5 rounded border border-dividerDark bg-primaryLight text-secondaryDark">
            {{ item.keys }}
          </span>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-end w-full">
        <HoppButtonSecondary label="关闭" outline @click="shortcutsModalOpen = false" />
      </div>
    </template>
  </HoppSmartModal>
</template>

<script setup lang="ts">
import { HoppButtonSecondary, HoppSmartModal } from "@hoppscotch/ui";
import { shortcutsModalOpen } from "../../stores/workspace";

const shortcuts = [
  { keys: "Ctrl / Cmd + Enter", desc: "立即发送单次调试请求 (Send Probe)" },
  { keys: "Ctrl / Cmd + S", desc: "保存当前标签页请求到集合 (Save Request)" },
  { keys: "Alt + N", desc: "新建 HTTP 请求标签页" },
  { keys: "Alt + M", desc: "新建 Mazbri 网关协议请求标签页" },
  { keys: "Ctrl / Cmd + /", desc: "打开 / 关闭快捷键面板" },
];
</script>
