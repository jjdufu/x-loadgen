<template>
  <input
    v-bind="$attrs"
    :value="modelValue ?? ''"
    :type="type"
    :disabled="disabled"
    :readonly="readonly"
    :placeholder="placeholder"
    :class="
      cn(
        'flex w-full rounded-md border border-input bg-background text-foreground shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
        size === 'sm' ? 'h-8 px-2.5 py-1 text-xs' : 'h-9 px-3 py-1.5 text-sm',
        mono && 'font-mono text-[13px]',
        props.class,
      )
    "
    @input="onInput"
  />
</template>

<script setup lang="ts">
import { cn } from "../../lib/utils";

defineOptions({
  inheritAttrs: false,
});

const props = withDefaults(
  defineProps<{
    modelValue?: string | number | null;
    type?: string;
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    mono?: boolean;
    size?: "default" | "sm";
    class?: string;
  }>(),
  {
    modelValue: "",
    type: "text",
    placeholder: undefined,
    disabled: false,
    readonly: false,
    mono: false,
    size: "default",
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}
</script>
