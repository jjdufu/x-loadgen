<template>
  <RouterView />

  <!-- Hoppscotch Toast Notification Stack -->
  <div
    class="fixed bottom-10 right-4 z-[10000] flex flex-col space-y-2 max-w-sm pointer-events-none"
  >
    <div
      v-for="item in activeToasts"
      :key="item.id"
      class="pointer-events-auto flex items-center justify-between space-x-3 px-4 py-2.5 rounded border shadow-lg text-tiny font-semibold transition"
      :class="toastKindClass(item.kind)"
    >
      <span class="break-words">{{ item.message }}</span>
      <button
        type="button"
        class="opacity-70 hover:opacity-100 text-xs flex-shrink-0"
        @click="dismissToast(item.id)"
      >
        ✕
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  activeToasts,
  dismissToast,
  type ToastKind,
} from "./modules/toast";

function toastKindClass(kind: ToastKind): string {
  if (kind === "error") {
    return "bg-rose-950/95 text-rose-200 border-rose-500/50";
  }
  if (kind === "success") {
    return "bg-emerald-950/95 text-emerald-200 border-emerald-500/50";
  }
  return "bg-popover text-secondaryDark border-accent";
}
</script>
