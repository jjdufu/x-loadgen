<template>
  <div class="secret-row">
    <template v-if="multiline">
      <el-input
        v-if="visible"
        :model-value="modelValue"
        type="textarea"
        :rows="rows"
        :placeholder="placeholder"
        autocomplete="off"
        spellcheck="false"
        class="mono"
        @update:model-value="onInput"
      />
      <el-input v-else :model-value="masked" readonly :placeholder="placeholder" class="masked" />
    </template>
    <el-input
      v-else
      :model-value="modelValue"
      :type="visible ? 'text' : 'password'"
      :placeholder="placeholder"
      autocomplete="off"
      @update:model-value="onInput"
    />
    <el-button :icon="visible ? Hide : View" :title="visible ? '隐藏' : '显示'" @click="visible = !visible" />
    <el-button :icon="DocumentCopy" title="复制" :disabled="!modelValue" @click="copy" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { ElMessage } from "element-plus";
import { DocumentCopy, Hide, View } from "@element-plus/icons-vue";
import { copyText } from "../utils/clipboard";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    /** PEM and other multi-line secrets: masked until revealed, then edited in a textarea. */
    multiline?: boolean;
    rows?: number;
  }>(),
  { placeholder: "", multiline: false, rows: 6 },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const visible = ref(false);

const masked = computed(() => {
  const value = props.modelValue ?? "";
  if (!value) return "";
  const lines = value.split("\n").length;
  return `${"•".repeat(24)}（已填写 ${lines} 行，点击右侧按钮显示）`;
});

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
  align-items: flex-start;
}
.secret-row :deep(.el-input),
.secret-row :deep(.el-textarea) {
  flex: 1;
}
.mono :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
.masked :deep(input) {
  color: #6b7280;
}
</style>
