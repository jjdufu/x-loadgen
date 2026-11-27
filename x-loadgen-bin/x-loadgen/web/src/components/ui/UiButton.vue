<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :class="cn(buttonVariants({ variant, size }), props.class)"
  >
    <Loader2 v-if="loading" class="h-3.5 w-3.5 animate-spin shrink-0" />
    <slot />
  </button>
</template>

<script setup lang="ts">
import { cva, type VariantProps } from "class-variance-authority";
import { Loader2 } from "lucide-vue-next";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "inline-flex cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 select-none",
  {
    variants: {
      variant: {
        default:
          "border border-transparent bg-primary text-primary-foreground shadow-sm hover:bg-primary-hover active:bg-primary-active",
        secondary:
          "border border-border bg-secondary text-secondary-foreground hover:bg-hover-bg hover:border-border-hover active:bg-active-bg",
        outline:
          "border border-input bg-background text-foreground shadow-sm hover:bg-hover-bg hover:border-border-hover hover:text-foreground active:bg-active-bg",
        ghost:
          "text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg",
        destructive:
          "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90 active:bg-destructive/85",
        link: "text-foreground underline-offset-4 hover:underline active:scale-100",
      },
      size: {
        default: "h-9 px-4 py-2 text-sm",
        sm: "h-8 rounded-md px-3 text-xs",
        xs: "h-7 rounded-md px-2.5 text-xs",
        lg: "h-10 rounded-md px-6 text-sm",
        icon: "h-9 w-9 rounded-md",
        "icon-sm": "h-8 w-8 rounded-md",
        "icon-xs": "h-7 w-7 rounded-md",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

type ButtonVariants = VariantProps<typeof buttonVariants>;

const props = withDefaults(
  defineProps<{
    variant?: ButtonVariants["variant"];
    size?: ButtonVariants["size"];
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
    loading?: boolean;
    class?: string;
  }>(),
  {
    variant: "default",
    size: "default",
    type: "button",
    disabled: false,
    loading: false,
    class: undefined,
  },
);
</script>
