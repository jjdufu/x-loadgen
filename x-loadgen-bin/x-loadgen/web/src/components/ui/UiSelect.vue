<template>
  <div class="relative inline-flex items-center" :class="wrapperClass">
    <SelectRoot
      :model-value="internalValue"
      :disabled="disabled"
      @update:model-value="onSelectValue"
    >
      <SelectTrigger
        v-bind="$attrs"
        :aria-label="ariaLabel"
        :class="
          cn(
            'flex w-full cursor-pointer items-center justify-between gap-1.5 rounded-md border border-input bg-background text-left text-foreground shadow-sm transition-colors select-none focus:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
            size === 'sm' ? 'h-8 px-2.5 text-xs' : 'h-9 px-3 text-sm',
            mono && 'font-mono text-[13px]',
            props.class,
          )
        "
      >
        <SelectValue :placeholder="placeholder">
          <span
            class="block truncate"
            :class="selectedOption?.triggerClass"
          >
            {{ selectedOption?.label ?? placeholder ?? "" }}
          </span>
        </SelectValue>
        <ChevronDown
          class="h-3.5 w-3.5 shrink-0 text-muted-foreground opacity-80"
        />
      </SelectTrigger>

      <SelectPortal>
        <SelectContent
          position="popper"
          :align="align"
          :side-offset="4"
          :class="
            cn(
              'z-[960] max-h-72 min-w-[var(--reka-select-trigger-width)] overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-lg focus:outline-none',
              contentClass,
            )
          "
          @close-auto-focus="guardAutoFocusEvent"
        >
          <SelectViewport class="max-h-72 overflow-y-auto p-1">
            <SelectItem
              v-for="opt in resolvedOptions"
              :key="toInternal(opt.value)"
              :value="toInternal(opt.value)"
              :disabled="opt.disabled"
              :text-value="opt.label"
              :class="
                cn(
                  'relative flex w-full cursor-pointer select-none items-center rounded-md py-1.5 pl-2.5 pr-7 text-xs text-popover-foreground outline-none transition-colors data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[state=checked]:font-medium data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
                  mono && 'font-mono text-[12px]',
                  opt.class,
                )
              "
            >
              <SelectItemText>{{ opt.label }}</SelectItemText>
              <SelectItemIndicator
                class="absolute right-2 inline-flex items-center justify-center text-foreground"
              >
                <Check class="h-3.5 w-3.5" />
              </SelectItemIndicator>
            </SelectItem>
          </SelectViewport>
        </SelectContent>
      </SelectPortal>
    </SelectRoot>
  </div>
</template>

<script setup lang="ts">
import {
  Comment,
  Fragment,
  computed,
  useSlots,
  type VNode,
  type VNodeArrayChildren,
} from "vue";
import {
  SelectContent,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectViewport,
} from "reka-ui";
import { Check, ChevronDown } from "lucide-vue-next";
import { cn } from "../../lib/utils";
import { guardAutoFocusEvent } from "../../modules/activity";

defineOptions({
  inheritAttrs: false,
});

export interface SelectOptionItem {
  value: string;
  label: string;
  disabled?: boolean;
  class?: string;
  triggerClass?: string;
}

const EMPTY_SENTINEL = "__xlg_empty_option__";

const props = withDefaults(
  defineProps<{
    modelValue?: string | number | null;
    options?: SelectOptionItem[];
    placeholder?: string;
    ariaLabel?: string;
    disabled?: boolean;
    mono?: boolean;
    size?: "default" | "sm";
    align?: "start" | "center" | "end";
    wrapperClass?: string;
    class?: string;
    contentClass?: string;
  }>(),
  {
    modelValue: "",
    options: undefined,
    placeholder: undefined,
    ariaLabel: undefined,
    disabled: false,
    mono: false,
    size: "default",
    align: "start",
    wrapperClass: "w-full",
    class: undefined,
    contentClass: undefined,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const slots = useSlots();

function toInternal(raw: string | number | null | undefined): string {
  const str = String(raw ?? "");
  return str === "" ? EMPTY_SENTINEL : str;
}

function fromInternal(internal: string): string {
  return internal === EMPTY_SENTINEL ? "" : internal;
}

function extractTextFromChildren(children: VNode["children"]): string {
  if (typeof children === "string" || typeof children === "number") {
    return String(children);
  }
  if (!Array.isArray(children)) {
    return "";
  }
  let out = "";
  for (const child of children as VNodeArrayChildren) {
    if (typeof child === "string" || typeof child === "number") {
      out += String(child);
    } else if (child && typeof child === "object" && "children" in child) {
      if (child.type !== Comment) {
        out += extractTextFromChildren(child.children);
      }
    }
  }
  return out;
}

function parseSlotOptions(vnodes: VNode[]): SelectOptionItem[] {
  const result: SelectOptionItem[] = [];
  for (const node of vnodes) {
    if (!node || node.type === Comment) continue;
    if (node.type === Fragment && Array.isArray(node.children)) {
      result.push(...parseSlotOptions(node.children as VNode[]));
      continue;
    }
    if (node.type === "option") {
      const p = (node.props ?? {}) as Record<string, unknown>;
      const label = extractTextFromChildren(node.children)
        .replace(/\s+/g, " ")
        .trim();
      const rawVal =
        "value" in p && p.value !== undefined && p.value !== null
          ? String(p.value)
          : label;
      const disabled = "disabled" in p && p.disabled !== false;
      result.push({
        value: rawVal,
        label,
        disabled,
      });
    }
  }
  return result;
}

const resolvedOptions = computed<SelectOptionItem[]>(() => {
  if (props.options && props.options.length > 0) {
    return props.options;
  }
  const slotNodes = slots.default?.() ?? [];
  return parseSlotOptions(slotNodes);
});

const normalizedModelValue = computed(() => String(props.modelValue ?? ""));

const internalValue = computed(() => toInternal(normalizedModelValue.value));

const selectedOption = computed<SelectOptionItem | undefined>(() => {
  return (
    resolvedOptions.value.find(
      (opt) => opt.value === normalizedModelValue.value,
    ) ?? resolvedOptions.value[0]
  );
});

function onSelectValue(nextInternal: unknown): void {
  if (typeof nextInternal !== "string") return;
  emit("update:modelValue", fromInternal(nextInternal));
}
</script>
