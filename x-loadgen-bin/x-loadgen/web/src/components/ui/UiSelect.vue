<template>
  <div class="relative inline-flex items-center" :class="wrapperClass">
    <select
      v-bind="$attrs"
      :value="modelValue ?? ''"
      :disabled="disabled"
      :class="
        cn(
          'w-full appearance-none rounded-md border border-input bg-background pr-8 text-foreground shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
          size === 'sm' ? 'h-8 pl-2.5 text-xs' : 'h-9 pl-3 text-sm',
          mono && 'font-mono text-[13px]',
          props.class,
        )
      "
      @change="onChange"
    >
      <slot />
    </select>
    <ChevronDown
      class="pointer-events-none absolute right-2.5 h-3.5 w-3.5 text-muted-foreground"
    />
  </div>
</template>

<script setup lang="ts">
import { ChevronDown } from "lucide-vue-next";
import { cn } from "../../lib/utils";

defineOptions({
  inheritAttrs: false,
});

const props = withDefaults(
  defineProps<{
    modelValue?: string | number | null;
    disabled?: boolean;
    mono?: boolean;
    size?: "default" | "sm";
    wrapperClass?: string;
    class?: string;
  }>(),
  {
    modelValue: "",
    disabled: false,
    mono: false,
    size: "default",
    wrapperClass: "w-full",
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

function onChange(event: Event): void {
  emit("update:modelValue", (event.target as HTMLSelectElement).value);
}
</script>
