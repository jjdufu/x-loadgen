<template>
  <UiDialog
    :open="Boolean(activeConfirm)"
    :title="activeConfirm?.title ?? '操作确认'"
    :description="activeConfirm?.description || undefined"
    max-width="sm"
    @close="resolveConfirm(false)"
  >
    <template #footer>
      <UiButton
        variant="outline"
        size="sm"
        @click="resolveConfirm(false)"
      >
        {{ activeConfirm?.cancelText ?? "取消" }}
      </UiButton>
      <UiButton
        :variant="activeConfirm?.variant ?? 'destructive'"
        size="sm"
        @click="resolveConfirm(true)"
      >
        {{ activeConfirm?.confirmText ?? "确认" }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { activeConfirm, resolveConfirm } from "../../modules/confirm";
import UiButton from "./UiButton.vue";
import UiDialog from "./UiDialog.vue";
</script>
