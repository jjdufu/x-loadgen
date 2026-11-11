<template>
  <div class="copy-row">
    <el-input :model-value="value" type="textarea" :rows="rows" readonly class="mono" />
    <el-button :icon="DocumentCopy" title="复制" :disabled="!value" @click="copy" />
  </div>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import { DocumentCopy } from "@element-plus/icons-vue";
import { copyText } from "../utils/clipboard";

const props = withDefaults(defineProps<{ value: string; rows?: number }>(), { rows: 5 });

async function copy(): Promise<void> {
  const ok = await copyText(props.value);
  if (ok) {
    ElMessage.success("已复制");
  } else {
    ElMessage.error("复制失败");
  }
}
</script>

<style scoped>
.copy-row {
  display: flex;
  gap: 8px;
  width: 100%;
  align-items: flex-start;
}
.copy-row :deep(.el-textarea) {
  flex: 1;
}
.mono :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
