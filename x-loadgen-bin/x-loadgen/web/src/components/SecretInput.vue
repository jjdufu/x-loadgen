<template>
  <div class="secret-row">
    <el-input
      :model-value="modelValue"
      :type="visible ? 'text' : 'password'"
      :placeholder="placeholder"
      autocomplete="off"
      @update:model-value="onInput"
    />
    <el-button :icon="visible ? Hide : View" :title="visible ? '隐藏' : '显示'" @click="visible = !visible" />
    <el-button :icon="DocumentCopy" title="复制" @click="copy" />
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { ElMessage } from "element-plus";
import { DocumentCopy, Hide, View } from "@element-plus/icons-vue";
import { copyText } from "../utils/clipboard";

const props = defineProps<{
  modelValue: string;
  placeholder?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const visible = ref(false);

function onInput(value: string): void {
  emit("update:modelValue", value);
}

async function copy(): Promise<void> {
  const ok = await copyText(props.modelValue ?? "");
  if (ok) {
    ElMessage.success("已复制");
  } else {
    ElMessage.error("复制失败");
  }
}
</script>

<style scoped>
.secret-row {
  display: flex;
  gap: 8px;
  width: 100%;
}
.secret-row :deep(.el-input) {
  flex: 1;
}
</style>
