<template>
  <div class="flex flex-1 flex-col">
    <div
      class="sticky top-0 z-10 flex items-center justify-between border-b border-border bg-surface/50 px-4 py-2"
    >
      <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        {{ title }}
      </span>
      <div class="flex items-center gap-1.5">
        <UiButton
          variant="ghost"
          size="xs"
          :title="bulkMode ? '切换回表格编辑' : '批量编辑 (key: value)'"
          @click="toggleBulkMode"
        >
          <FileEdit class="h-3.5 w-3.5" />
          <span>{{ bulkMode ? "表格模式" : "批量编辑" }}</span>
        </UiButton>
        <UiButton
          v-if="!bulkMode && rows.length > 0"
          variant="ghost"
          size="xs"
          title="清空全部条目"
          @click="clearAll"
        >
          <Trash2 class="h-3.5 w-3.5" />
          <span>清空</span>
        </UiButton>
        <UiButton
          v-if="!bulkMode"
          variant="outline"
          size="xs"
          @click="addRow"
        >
          <Plus class="h-3.5 w-3.5" />
          <span>新增参数</span>
        </UiButton>
      </div>
    </div>

    <div v-if="bulkMode" class="flex flex-col gap-2 p-4">
      <p class="text-xs text-muted-foreground">
        每行一条 <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">key: value</code>，以
        <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">#</code> 开头表示暂时禁用该行。
      </p>
      <UiTextarea
        v-model="bulkText"
        :rows="8"
        mono
        placeholder="Content-Type: application/json&#10;# X-Debug: 1"
        @blur="applyBulkText"
      />
    </div>

    <div
      v-else-if="rows.length === 0"
      class="flex flex-col items-center justify-center py-10 text-muted-foreground"
    >
      <p class="mb-3 text-sm">{{ emptyText }}</p>
      <UiButton variant="outline" size="sm" @click="addRow">
        <Plus class="h-3.5 w-3.5" />
        <span>添加第一行</span>
      </UiButton>
    </div>

    <div v-else class="divide-y divide-border border-b border-border">
      <div
        v-for="(row, index) in rows"
        :key="index"
        class="group flex items-center divide-x divide-border transition-colors hover:bg-muted/35"
      >
        <div class="flex h-9 w-10 shrink-0 items-center justify-center">
          <input
            type="checkbox"
            :checked="row.active"
            :aria-label="`启用第 ${index + 1} 行`"
            class="h-4 w-4 cursor-pointer rounded border-input accent-foreground"
            @change="toggleRow(index)"
          />
        </div>
        <div class="min-w-0 flex-1">
          <input
            :value="row.key"
            type="text"
            :placeholder="keyPlaceholder"
            class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
            :class="{ 'opacity-50 line-through': !row.active }"
            @input="updateField(index, 'key', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div class="min-w-0 flex-1">
          <input
            :value="row.value"
            type="text"
            :placeholder="valuePlaceholder"
            class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
            :class="{ 'opacity-50': !row.active }"
            @input="updateField(index, 'value', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div v-if="showDescription" class="hidden min-w-0 flex-1 md:block">
          <input
            :value="row.description ?? ''"
            type="text"
            placeholder="备注说明 (可选)"
            class="h-9 w-full bg-transparent px-3 text-xs text-muted-foreground placeholder:text-muted-foreground/70 focus:text-foreground focus:outline-none"
            @input="updateField(index, 'description', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div class="flex h-9 w-10 shrink-0 items-center justify-center">
          <button
            type="button"
            title="删除此行"
            aria-label="删除此行"
            class="rounded p-1 text-muted-foreground transition-colors hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
            @click="removeRow(index)"
          >
            <Trash2 class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { FileEdit, Plus, Trash2 } from "lucide-vue-next";
import type { KeyValueRow } from "../../api/types";
import { createEmptyKvRow } from "../../helpers/format";
import UiButton from "../ui/UiButton.vue";
import UiTextarea from "../ui/UiTextarea.vue";

const props = withDefaults(
  defineProps<{
    modelValue: KeyValueRow[];
    title?: string;
    keyPlaceholder?: string;
    valuePlaceholder?: string;
    emptyText?: string;
    showDescription?: boolean;
  }>(),
  {
    title: "参数列表",
    keyPlaceholder: "Key (支持 <<var>>)",
    valuePlaceholder: "Value (支持 <<var>> / {{seq}})",
    emptyText: "当前列表为空",
    showDescription: true,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: KeyValueRow[]];
}>();

const bulkMode = ref(false);
const bulkText = ref("");

const rows = computed(() => props.modelValue ?? []);

function serializeRows(list: KeyValueRow[]): string {
  return list
    .map((r) => `${r.active ? "" : "# "}${r.key}: ${r.value}`)
    .join("\n");
}

function parseBulkRows(raw: string): KeyValueRow[] {
  return raw
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .map((line) => {
      const active = !line.startsWith("#");
      const clean = active ? line : line.replace(/^#\s*/, "");
      const idx = clean.indexOf(":");
      if (idx < 0) {
        return { key: clean.trim(), value: "", active };
      }
      return {
        key: clean.slice(0, idx).trim(),
        value: clean.slice(idx + 1).trim(),
        active,
      };
    });
}

function toggleBulkMode(): void {
  if (!bulkMode.value) {
    bulkText.value = serializeRows(rows.value);
    bulkMode.value = true;
  } else {
    applyBulkText();
    bulkMode.value = false;
  }
}

function applyBulkText(): void {
  emit("update:modelValue", parseBulkRows(bulkText.value));
}

function addRow(): void {
  emit("update:modelValue", [...rows.value, createEmptyKvRow()]);
}

function removeRow(index: number): void {
  const next = rows.value.slice();
  next.splice(index, 1);
  emit("update:modelValue", next);
}

function clearAll(): void {
  emit("update:modelValue", []);
}

function toggleRow(index: number): void {
  const next = rows.value.map((r, i) =>
    i === index ? { ...r, active: !r.active } : r,
  );
  emit("update:modelValue", next);
}

function updateField(
  index: number,
  field: "key" | "value" | "description",
  val: string,
): void {
  const next = rows.value.map((r, i) =>
    i === index ? { ...r, [field]: val } : r,
  );
  emit("update:modelValue", next);
}
</script>
