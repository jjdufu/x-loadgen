<template>
  <div :class="cn('relative flex items-center', wrapperClass)">
    <Search
      class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
    />
    <UiInput
      v-bind="$attrs"
      :model-value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :size="size"
      :class="cn('pl-8', hasValue && 'pr-7', props.class)"
      @update:model-value="emit('update:modelValue', $event)"
    />
    <button
      v-if="hasValue && !disabled"
      type="button"
      class="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      title="清空搜索"
      aria-label="清空搜索"
      @click="emit('update:modelValue', '')"
    >
      <X class="h-3 w-3" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { Search, X } from "lucide-vue-next";
import { cn } from "../../lib/utils";
import UiInput from "./UiInput.vue";

defineOptions({
  inheritAttrs: false,
});

const props = withDefaults(
  defineProps<{
    modelValue?: string | null;
    placeholder?: string;
    disabled?: boolean;
    size?: "default" | "sm";
    wrapperClass?: string;
    class?: string;
  }>(),
  {
    modelValue: "",
    placeholder: "搜索...",
    disabled: false,
    size: "sm",
    wrapperClass: "w-full",
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const hasValue = computed(() => Boolean(props.modelValue && props.modelValue.length > 0));
</script>
