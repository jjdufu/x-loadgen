<template>
  <UiDialog
    :open="shortcutsModalOpen"
    title="键盘快捷键指南"
    description="在接口调试与压测工作台中使用以下组合键提升操作效率。"
    max-width="md"
    @close="shortcutsModalOpen = false"
  >
    <div class="divide-y divide-border overflow-hidden rounded-lg border border-border">
      <div
        v-for="item in shortcuts"
        :key="item.keys"
        class="flex items-center justify-between gap-4 px-4 py-3 text-sm"
      >
        <span class="text-foreground">{{ item.desc }}</span>
        <kbd
          class="shrink-0 rounded-md border border-border bg-muted px-2 py-0.5 font-mono text-xs font-medium text-foreground"
        >
          {{ item.keys }}
        </kbd>
      </div>
    </div>

    <template #footer>
      <UiButton variant="outline" @click="shortcutsModalOpen = false">
        关闭
      </UiButton>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { shortcutsModalOpen } from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiDialog from "../ui/UiDialog.vue";

const shortcuts = [
  { keys: "Ctrl / Cmd + Enter", desc: "发送单次接口调试请求" },
  { keys: "Ctrl / Cmd + S", desc: "保存当前标签页请求到集合" },
  { keys: "Alt + N", desc: "新建 HTTP 请求标签页" },
  { keys: "Alt + M", desc: "新建 Mazbri 网关协议标签页" },
  { keys: "Ctrl / Cmd + /", desc: "打开 / 关闭快捷键面板" },
];
</script>
