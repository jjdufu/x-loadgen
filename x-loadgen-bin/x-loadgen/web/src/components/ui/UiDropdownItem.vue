<template>
  <DropdownMenuItem
    :disabled="disabled"
    :class="
      cn(
        'relative flex cursor-pointer select-none items-center gap-2 rounded-md px-2.5 py-1.5 text-sm outline-none transition-colors data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
        destructive
          ? 'text-rose-600 dark:text-rose-400 focus:bg-rose-500/10'
          : 'text-foreground focus:bg-accent focus:text-accent-foreground',
        props.class,
      )
    "
    @select="emit('select')"
  >
    <slot />
  </DropdownMenuItem>
</template>

<script setup lang="ts">
import { DropdownMenuItem } from "reka-ui";
import { cn } from "../../lib/utils";

const props = withDefaults(
  defineProps<{
    disabled?: boolean;
    destructive?: boolean;
    class?: string;
  }>(),
  {
    disabled: false,
    destructive: false,
    class: undefined,
  },
);

const emit = defineEmits<{
  (e: "select"): void;
}>();
</script>
