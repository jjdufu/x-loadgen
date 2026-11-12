<template>
  <div
    class="flex flex-wrap items-center justify-between gap-2 px-4 py-1.5 border-b border-dividerLight bg-primary text-tiny"
  >
    <!-- Breadcrumb + Inline Editable Request Name -->
    <div class="flex items-center space-x-1.5 min-w-0 flex-1">
      <Folder class="w-3.5 h-3.5 text-secondaryLight flex-shrink-0" />
      <template v-if="breadcrumb && breadcrumb.length">
        <span
          v-for="(seg, idx) in breadcrumb"
          :key="idx"
          class="text-secondaryLight truncate max-w-[140px]"
        >
          {{ seg }} /
        </span>
      </template>
      <span v-else class="text-secondaryLight">未归档草稿 /</span>

      <input
        :value="draft.name"
        type="text"
        placeholder="输入请求名称..."
        class="px-1.5 py-0.5 rounded bg-transparent hover:bg-primaryLight focus:bg-primaryLight font-semibold text-secondaryDark min-w-[160px] max-w-xs"
        @input="draft.name = ($event.target as HTMLInputElement).value"
      />

      <span
        v-if="dirty"
        v-tippy="'当前标签页有未保存修改'"
        class="w-2 h-2 rounded-full bg-amber-500 flex-shrink-0"
      />
    </div>

    <!-- Hoppscotch Dual-Protocol Switcher (HTTP vs Mazbri) -->
    <div class="flex items-center space-x-3 flex-shrink-0">
      <div class="flex items-center space-x-1.5 text-secondaryLight">
        <span>集合权重</span>
        <input
          :value="draft.weight"
          type="number"
          min="1"
          class="w-14 px-2 py-0.5 rounded border border-divider bg-primaryLight font-mono text-secondaryDark text-right"
          @input="draft.weight = Math.max(1, Number(($event.target as HTMLInputElement).value) || 1)"
        />
      </div>

      <div
        class="inline-flex items-center rounded border border-divider bg-primaryDark p-0.5"
        role="group"
        aria-label="选择请求协议"
      >
        <button
          type="button"
          class="flex items-center space-x-1.5 px-2.5 py-1 rounded font-semibold transition"
          :class="
            draft.protocol === 'http'
              ? 'bg-accent text-accentContrast shadow-sm'
              : 'text-secondary hover:text-secondaryDark'
          "
          @click="switchProtocol('http')"
        >
          <Globe class="w-3.5 h-3.5" />
          <span>HTTP 通用协议</span>
        </button>
        <button
          type="button"
          class="flex items-center space-x-1.5 px-2.5 py-1 rounded font-semibold transition"
          :class="
            draft.protocol === 'mazbri'
              ? 'bg-accent text-accentContrast shadow-sm'
              : 'text-secondary hover:text-secondaryDark'
          "
          @click="switchProtocol('mazbri')"
        >
          <ShieldCheck class="w-3.5 h-3.5" />
          <span>Mazbri 加密网关</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { Folder, Globe, ShieldCheck } from "lucide-vue-next";
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

function switchProtocol(next: "http" | "mazbri"): void {
  if (draft.value.protocol === next) return;
  draft.value.protocol = next;
  if (next === "mazbri") {
    activeTab.value.requestSubTab = "auth";
    if (draft.value.endpoint.startsWith("http")) {
      draft.value.endpoint = "/v1/demo";
    }
    if (draft.value.method === "GET" && !draft.value.body.content_type) {
      draft.value.method = "POST";
      draft.value.body.content_type = "application/json";
      if (!draft.value.body.body) {
        draft.value.body.body = '{\n  "ping": "mazbri"\n}';
      }
    }
  }
}
</script>
