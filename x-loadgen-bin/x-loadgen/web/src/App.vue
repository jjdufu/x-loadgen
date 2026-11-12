<template>
  <RouterView />
  <UiConfirmDialog />

  <!-- Shadcn / Sonner-inspired Swiss Minimalist Toast Stack -->
  <div
    class="fixed bottom-10 right-4 z-[10000] flex flex-col gap-2.5 w-full max-w-[360px] pointer-events-none"
    aria-live="polite"
  >
    <TransitionGroup
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="translate-y-2 opacity-0 scale-95"
      enter-to-class="translate-y-0 opacity-100 scale-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0 scale-95"
    >
      <div
        v-for="item in activeToasts"
        :key="item.id"
        class="pointer-events-auto flex items-start gap-3 rounded-lg border bg-popover px-4 py-3 text-popover-foreground shadow-lg"
        :class="toastBorderClass(item.kind)"
      >
        <component
          :is="toastIcon(item.kind)"
          class="mt-0.5 h-4 w-4 shrink-0"
          :class="toastIconClass(item.kind)"
        />
        <span class="flex-1 text-sm leading-snug break-words">
          {{ item.message }}
        </span>
        <button
          type="button"
          class="rounded p-0.5 text-muted-foreground hover:text-foreground transition-colors shrink-0"
          aria-label="关闭通知"
          @click="dismissToast(item.id)"
        >
          <X class="h-3.5 w-3.5" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { AlertCircle, CheckCircle2, Info, X } from "lucide-vue-next";
import UiConfirmDialog from "./components/ui/UiConfirmDialog.vue";
import {
  activeToasts,
  dismissToast,
  type ToastKind,
} from "./modules/toast";

function toastIcon(kind: ToastKind) {
  if (kind === "error") return AlertCircle;
  if (kind === "success") return CheckCircle2;
  return Info;
}

function toastIconClass(kind: ToastKind): string {
  if (kind === "error") return "text-rose-600 dark:text-rose-400";
  if (kind === "success") return "text-emerald-600 dark:text-emerald-400";
  return "text-foreground";
}

function toastBorderClass(kind: ToastKind): string {
  if (kind === "error") return "border-rose-500/35";
  return "border-border";
}
</script>
