<template>
  <div class="flex flex-col flex-1">
    <div
      class="sticky top-0 z-10 flex items-center justify-between pl-4 pr-2 py-1 border-b border-dividerLight bg-primary"
    >
      <span class="font-semibold text-secondaryLight text-tiny uppercase tracking-wider">
        {{ title }}
      </span>
      <div class="flex items-center space-x-1">
        <HoppButtonSecondary
          v-tippy="bulkMode ? '切换回表格编辑' : '批量编辑 (key: value)'"
          :icon="FileEdit"
          :label="bulkMode ? '表格模式' : '批量编辑'"
          @click="toggleBulkMode"
        />
        <HoppButtonSecondary
          v-if="!bulkMode"
          v-tippy="'清空全部条目'"
          :icon="Trash2"
          @click="clearAll"
        />
        <HoppButtonSecondary
          v-if="!bulkMode"
          v-tippy="'新增一行'"
          :icon="Plus"
          label="添加"
          @click="addRow"
        />
      </div>
    </div>

    <div v-if="bulkMode" class="p-3 flex flex-col space-y-2">
      <p class="text-tiny text-secondaryLight">
        每行一条 <code class="font-mono text-accent">key: value</code>，以 <code class="font-mono">#</code> 开头表示暂时禁用该行。
      </p>
      <textarea
        v-model="bulkText"
        rows="8"
        class="textarea font-mono text-body bg-primaryLight"
        placeholder="Content-Type: application/json&#10;# X-Debug: 1"
        @blur="applyBulkText"
      />
    </div>

    <div v-else-if="rows.length === 0" class="flex flex-col items-center justify-center py-8 text-secondaryLight">
      <p class="text-tiny mb-3">{{ emptyText }}</p>
      <HoppButtonSecondary :icon="Plus" label="新增参数" outline @click="addRow" />
    </div>

    <div v-else class="divide-y divide-dividerLight border-b border-dividerLight">
      <div
        v-for="(row, index) in rows"
        :key="index"
        class="flex items-center divide-x divide-dividerLight group hover:bg-primaryLight/60 transition"
      >
        <div class="flex items-center justify-center w-9 py-1.5 flex-shrink-0">
          <HoppSmartCheckbox
            :on="row.active"
            :name="`kv-active-${index}`"
            @change="toggleRow(index)"
          />
        </div>
        <div class="flex-1 min-w-0">
          <input
            :value="row.key"
            type="text"
            :placeholder="keyPlaceholder"
            class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark placeholder-secondaryLight"
            :class="{ 'opacity-50 line-through': !row.active }"
            @input="updateField(index, 'key', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div class="flex-1 min-w-0">
          <input
            :value="row.value"
            type="text"
            :placeholder="valuePlaceholder"
            class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark placeholder-secondaryLight"
            :class="{ 'opacity-50': !row.active }"
            @input="updateField(index, 'value', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div v-if="showDescription" class="flex-1 min-w-0 hidden md:block">
          <input
            :value="row.description ?? ''"
            type="text"
            placeholder="说明 (可选)"
            class="w-full px-3 py-1.5 bg-transparent text-secondary placeholder-secondaryLight"
            @input="updateField(index, 'description', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div class="flex items-center justify-center w-9 flex-shrink-0">
          <HoppButtonSecondary
            v-tippy="'删除此行'"
            :icon="Trash"
            color="red"
            @click="removeRow(index)"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { HoppButtonSecondary, HoppSmartCheckbox } from "@hoppscotch/ui";
import { FileEdit, Plus, Trash, Trash2 } from "lucide-vue-next";
import type { KeyValueRow } from "../../api/types";
import { createEmptyKvRow } from "../../helpers/format";

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
