<template>
  <DialogRoot :open="open" @update:open="onOpenChange">
    <DialogPortal>
      <DialogOverlay
        class="fixed inset-0 z-[900] bg-black/65 backdrop-blur-[1.5px] transition-opacity"
      />
      <DialogContent
        :class="
          cn(
            'fixed left-1/2 top-1/2 z-[910] flex max-h-[88vh] w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl border border-border bg-background text-foreground shadow-2xl focus:outline-none',
            maxWidthClass,
            props.class,
          )
        "
        @open-auto-focus="guardAutoFocusEvent"
        @close-auto-focus="guardAutoFocusEvent"
      >
        <div
          class="flex items-start justify-between gap-4 border-b border-border px-6 py-4 shrink-0"
        >
          <div class="min-w-0 space-y-1">
            <DialogTitle class="text-base font-semibold tracking-tight text-foreground">
              {{ title }}
            </DialogTitle>
            <DialogDescription
              v-if="description"
              class="text-xs text-muted-foreground leading-relaxed"
            >
              {{ description }}
            </DialogDescription>
          </div>
          <DialogClose
            type="button"
            class="cursor-pointer rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            aria-label="关闭弹窗"
          >
            <X class="h-4 w-4" />
          </DialogClose>
        </div>

        <div
          v-if="$slots.default"
          class="flex-1 overflow-y-auto px-6 py-5"
        >
          <slot />
        </div>

        <div
          v-if="$slots.footer"
          class="flex items-center justify-end gap-2.5 border-t border-border bg-muted/25 px-6 py-3.5 shrink-0"
        >
          <slot name="footer" />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";
import { X } from "lucide-vue-next";
import { cn } from "../../lib/utils";
import { guardAutoFocusEvent } from "../../modules/activity";

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    description?: string;
    maxWidth?: "sm" | "md" | "lg" | "xl" | "2xl" | "3xl" | "4xl" | "5xl";
    class?: string;
  }>(),
  {
    description: undefined,
    maxWidth: "lg",
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:open", value: boolean): void;
  (e: "close"): void;
}>();

const maxWidthClass = computed(() => {
  const map: Record<string, string> = {
    sm: "max-w-sm",
    md: "max-w-md",
    lg: "max-w-lg",
    xl: "max-w-xl",
    "2xl": "max-w-2xl",
    "3xl": "max-w-3xl",
    "4xl": "max-w-4xl",
    "5xl": "max-w-5xl",
  };
  return map[props.maxWidth] ?? "max-w-lg";
});

function onOpenChange(next: boolean): void {
  emit("update:open", next);
  if (!next) {
    emit("close");
  }
}
</script>
