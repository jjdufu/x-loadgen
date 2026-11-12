<template>
  <textarea
    v-bind="$attrs"
    :value="modelValue ?? ''"
    :rows="rows"
    :disabled="disabled"
    :readonly="readonly"
    :placeholder="placeholder"
    :class="
      cn(
        'flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
        mono && 'font-mono text-[13px] leading-relaxed',
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
    modelValue?: string | null;
    rows?: number;
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    mono?: boolean;
    class?: string;
  }>(),
  {
    modelValue: "",
    rows: 3,
    placeholder: undefined,
    disabled: false,
    readonly: false,
    mono: false,
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLTextAreaElement).value);
}
</script>
