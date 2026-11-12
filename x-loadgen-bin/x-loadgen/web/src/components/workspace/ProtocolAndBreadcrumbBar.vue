<template>
  <div
    class="flex items-center justify-between gap-3 px-4 pt-2.5 pb-1 bg-background text-xs"
  >
    <!-- Breadcrumb + Inline Editable Request Name -->
    <div class="flex min-w-0 flex-1 items-center gap-1.5">
      <Folder class="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <template v-if="breadcrumb && breadcrumb.length">
        <span
          v-for="(seg, idx) in breadcrumb"
          :key="idx"
          class="max-w-[140px] truncate text-muted-foreground"
        >
          {{ seg }} /
        </span>
      </template>
      <span v-else class="whitespace-nowrap text-muted-foreground">未归档草稿 /</span>

      <input
        :value="draft.name"
        type="text"
        aria-label="请求名称"
        placeholder="输入请求名称..."
        class="min-w-[160px] max-w-xs rounded px-1.5 py-0.5 text-sm font-medium text-foreground transition-colors hover:bg-muted focus:bg-muted focus:outline-none"
        @input="draft.name = ($event.target as HTMLInputElement).value"
      />

      <span
        v-if="dirty"
        title="当前标签页存在未保存更改"
        class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
      />
    </div>

    <!-- Subtle Collection Weight Pill -->
    <div class="hidden sm:flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
      <span class="whitespace-nowrap">集合权重</span>
      <input
        :value="draft.weight"
        type="number"
        min="1"
        aria-label="集合权重"
        class="h-7 w-14 rounded-md border border-input bg-background px-2 text-right font-mono text-xs text-foreground tabular-nums focus:outline-none focus:ring-1 focus:ring-ring"
        @input="
          draft.weight = Math.max(
            1,
            Number(($event.target as HTMLInputElement).value) || 1,
          )
        "
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { Folder } from "lucide-vue-next";
import {
  activeTab,
  collections,
  findCollectionBreadcrumb,
  isTabDirty,
} from "../../stores/workspace";

const draft = computed(() => activeTab.value.draft);
const dirty = computed(() => isTabDirty(activeTab.value));
const breadcrumb = computed(() =>
  findCollectionBreadcrumb(collections.value, draft.value.collection_id),
);
</script>
