<template>
  <div class="relative flex items-center space-x-1 px-2 flex-shrink-0">
    <Layers class="w-3.5 h-3.5 text-accent flex-shrink-0" />
    <select
      :value="activeEnvironmentId ?? ''"
      aria-label="选择运行环境"
      class="bg-transparent text-secondaryDark font-semibold text-tiny py-1 pr-6 pl-1 cursor-pointer border border-transparent hover:border-divider rounded"
      @change="onSelectEnv(($event.target as HTMLSelectElement).value)"
    >
      <option value="">无环境 (No Environment)</option>
      <option v-for="env in selectableEnvironments" :key="env.id" :value="env.id">
        {{ env.name }}
      </option>
    </select>

    <HoppButtonSecondary
      v-tippy="'环境速览与编辑 (Environment Quick Peek)'"
      :icon="Eye"
      @click="peekOpen = !peekOpen"
    />

    <HoppButtonSecondary
      v-tippy="'新建环境 / Mazbri 密钥池'"
      :icon="Plus"
      @click="openEnvironmentEditor(null)"
    />

    <!-- Quick Peek Popover -->
    <div
      v-if="peekOpen"
      class="absolute right-2 top-9 z-40 w-80 rounded border border-dividerDark bg-popover shadow-xl p-3 space-y-3 text-tiny"
    >
      <div class="flex items-center justify-between border-b border-dividerLight pb-2">
        <span class="font-bold text-secondaryDark">
          环境速览：{{ activeEnvironmentDetail?.name ?? "仅 Global 全局变量" }}
        </span>
        <button
          type="button"
          class="text-secondaryLight hover:text-secondaryDark"
          @click="peekOpen = false"
        >
          ✕
        </button>
      </div>

      <div v-if="activeEnvironmentDetail" class="space-y-1 text-secondary">
        <div class="flex items-center justify-between">
          <span>默认网关：</span>
          <span class="font-mono text-secondaryDark truncate max-w-[180px]">
            {{ activeEnvironmentDetail.gateway_base_url || "未设置" }}
          </span>
        </div>
        <div class="flex items-center justify-between">
          <span>Mazbri 租户池：</span>
          <span class="font-mono text-accent font-semibold">
            {{ activeEnvironmentDetail.tenants.length }} 个租户
          </span>
        </div>
      </div>

      <div class="space-y-1 max-h-40 overflow-y-auto">
        <div class="font-semibold text-secondaryLight uppercase">生效变量 (&lt;&lt;var&gt;&gt;)</div>
        <div v-if="effectiveVariables.length === 0" class="text-secondaryLight py-2">
          当前暂无生效变量
        </div>
        <div
          v-for="v in effectiveVariables"
          :key="v.key"
          class="flex items-center justify-between py-0.5 font-mono"
        >
          <span class="text-accent">&lt;&lt;{{ v.key }}&gt;&gt;</span>
          <span class="text-secondaryDark truncate max-w-[140px]">
            {{ v.secret ? "••••••••" : v.value }}
          </span>
        </div>
      </div>

      <div class="flex items-center justify-between pt-2 border-t border-dividerLight">
        <HoppButtonSecondary
          label="编辑 Global"
          outline
          @click="editGlobal"
        />
        <HoppButtonSecondary
          v-if="activeEnvironmentId"
          label="编辑当前环境"
          outline
          @click="editActive"
        />
        <HoppButtonPrimary
          v-else
          label="新建环境"
          @click="createNew"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { HoppButtonPrimary, HoppButtonSecondary } from "@hoppscotch/ui";
import { Eye, Layers, Plus } from "lucide-vue-next";
import {
  activeEnvironmentDetail,
  activeEnvironmentId,
  effectiveVariables,
  globalEnvironmentDetail,
  openEnvironmentEditor,
  selectableEnvironments,
} from "../../stores/workspace";

const peekOpen = ref(false);

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
