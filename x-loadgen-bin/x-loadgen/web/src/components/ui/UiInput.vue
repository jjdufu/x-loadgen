<template>
  <input
    v-bind="forwardedAttrs"
    :value="displayValue"
    :type="type"
    :disabled="disabled"
    :readonly="readonly"
    :placeholder="placeholder"
    :autocomplete="resolvedAutocomplete"
    :class="
      cn(
        'flex w-full rounded-md border border-input bg-background text-foreground shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
        size === 'sm' ? 'h-8 px-2.5 py-1 text-xs' : 'h-9 px-3 py-1.5 text-sm',
        mono && 'font-mono text-[13px]',
        props.class,
      )
    "
    @focus="onFocus"
    @blur="onBlur"
    @input="onInput"
    @keydown.enter="onEnterKey"
  />
</template>

<script setup lang="ts">
import { computed, ref, useAttrs, watch } from "vue";
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
    autocomplete?: string;
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
    autocomplete: undefined,
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "blur", event: Event): void;
  (e: "focus", event: Event): void;
}>();

const attrs = useAttrs();
const focused = ref(false);
const draftText = ref(String(props.modelValue ?? ""));

const forwardedAttrs = computed(() => {
  const rest = { ...attrs };
  delete rest.autocomplete;
  return rest;
});

const resolvedAutocomplete = computed(() => {
  if (props.autocomplete !== undefined) return props.autocomplete;
  if (typeof attrs.autocomplete === "string") return attrs.autocomplete;
  return "off";
});

const displayValue = computed(() => {
  if (focused.value && props.type === "number") {
    return draftText.value;
  }
  return String(props.modelValue ?? "");
});

watch(
  () => props.modelValue,
  (next) => {
    if (!focused.value) {
      draftText.value = String(next ?? "");
    }
  },
);

function onFocus(event: Event): void {
  focused.value = true;
  draftText.value = String(props.modelValue ?? "");
  emit("focus", event);
}

function onBlur(event: Event): void {
  if (props.type === "number" && draftText.value.trim() === "") {
    emit("update:modelValue", "");
  }
  focused.value = false;
  draftText.value = String(props.modelValue ?? "");
  emit("blur", event);
}

function onEnterKey(): void {
  if (props.type === "number" && draftText.value.trim() === "") {
    emit("update:modelValue", "");
  }
}

function onInput(event: Event): void {
  const raw = (event.target as HTMLInputElement).value;
  draftText.value = raw;
  if (props.type === "number" && raw.trim() === "") {
    return;
  }
  emit("update:modelValue", raw);
}
</script>
