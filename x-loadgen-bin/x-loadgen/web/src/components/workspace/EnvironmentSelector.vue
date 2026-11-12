<template>
  <div ref="rootEl" class="relative flex items-center gap-1.5 px-3 py-1 shrink-0 border-l border-border">
    <Layers class="h-3.5 w-3.5 text-muted-foreground shrink-0" />
    <UiSelect
      :model-value="activeEnvironmentId ?? ''"
      size="sm"
      aria-label="选择运行环境"
      wrapper-class="w-44"
      class="border-transparent bg-transparent shadow-none hover:bg-accent/60 font-medium"
      @update:model-value="onSelectEnv"
    >
      <option value="">无环境</option>
      <option v-for="env in selectableEnvironments" :key="env.id" :value="env.id">
        {{ env.name }}
      </option>
    </UiSelect>

    <UiButton
      variant="ghost"
      size="icon-xs"
      title="变量速览"
      aria-label="变量速览"
      @click="peekOpen = !peekOpen"
    >
      <Eye class="h-3.5 w-3.5" />
    </UiButton>

    <UiButton
      variant="ghost"
      size="icon-xs"
      title="新建环境"
      aria-label="新建环境"
      @click="openEnvironmentEditor(null)"
    >
      <Plus class="h-3.5 w-3.5" />
    </UiButton>

    <!-- Quick Peek Popover -->
    <div
      v-if="peekOpen"
      class="absolute right-3 top-10 z-50 w-80 rounded-lg border border-border bg-popover p-4 text-popover-foreground shadow-xl space-y-3"
    >
      <div class="flex items-center justify-between border-b border-border pb-2.5">
        <span class="text-sm font-semibold truncate pr-2">
          {{ activeEnvironmentDetail?.name ?? "全局变量" }}
        </span>
        <button
          type="button"
          class="rounded p-0.5 text-muted-foreground hover:text-foreground"
          @click="peekOpen = false"
        >
          <X class="h-3.5 w-3.5" />
        </button>
      </div>

      <div v-if="activeEnvironmentDetail" class="space-y-1.5 text-xs">
        <div class="flex items-center justify-between gap-2">
          <span class="text-muted-foreground">默认网关</span>
          <span class="font-mono text-foreground truncate max-w-[185px]">
            {{ activeEnvironmentDetail.gateway_base_url || "未设置" }}
          </span>
        </div>
        <div class="flex items-center justify-between gap-2">
          <span class="text-muted-foreground">Mazbri 租户池</span>
          <span class="font-mono font-medium text-foreground">
            {{ activeEnvironmentDetail.tenants.length }} 个租户
          </span>
        </div>
      </div>

      <div class="space-y-1.5 max-h-44 overflow-y-auto pt-1">
        <div class="text-[11px] font-medium text-muted-foreground">
          生效模板变量 (&lt;&lt;var&gt;&gt;)
        </div>
        <div v-if="effectiveVariables.length === 0" class="py-2 text-xs text-muted-foreground">
          暂无生效变量
        </div>
        <div
          v-for="v in effectiveVariables"
          :key="v.key"
          class="flex items-center justify-between gap-2 py-0.5 font-mono text-xs"
        >
          <span class="text-foreground">&lt;&lt;{{ v.key }}&gt;&gt;</span>
          <span class="text-muted-foreground truncate max-w-[145px]">
            {{ v.secret ? "••••••••" : v.value }}
          </span>
        </div>
      </div>

      <div class="flex items-center justify-between gap-2 border-t border-border pt-3">
        <UiButton variant="outline" size="sm" @click="editGlobal">
          全局变量
        </UiButton>
        <UiButton
          v-if="activeEnvironmentId"
          variant="default"
          size="sm"
          @click="editActive"
        >
          编辑环境
        </UiButton>
        <UiButton v-else variant="default" size="sm" @click="createNew">
          新建环境
        </UiButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { Eye, Layers, Plus, X } from "lucide-vue-next";
import {
  activeEnvironmentDetail,
  activeEnvironmentId,
  effectiveVariables,
  globalEnvironmentDetail,
  openEnvironmentEditor,
  selectableEnvironments,
} from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiSelect from "../ui/UiSelect.vue";

const peekOpen = ref(false);
const rootEl = ref<HTMLDivElement | null>(null);

function handleOutsideClick(ev: MouseEvent): void {
  if (!peekOpen.value || !rootEl.value) return;
  if (!rootEl.value.contains(ev.target as Node)) {
    peekOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener("mousedown", handleOutsideClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("mousedown", handleOutsideClick);
});

function onSelectEnv(val: string): void {
  activeEnvironmentId.value = val || null;
}

function editGlobal(): void {
  peekOpen.value = false;
  openEnvironmentEditor(globalEnvironmentDetail.value?.id ?? "global");
}

function editActive(): void {
  peekOpen.value = false;
  if (activeEnvironmentId.value) {
    openEnvironmentEditor(activeEnvironmentId.value);
  }
}

function createNew(): void {
  peekOpen.value = false;
  openEnvironmentEditor(null);
}
</script>
