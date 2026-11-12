import { ref } from "vue";

export type ToastKind = "info" | "success" | "error";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

export const activeToasts = ref<ToastItem[]>([]);
let nextToastId = 1;

export function pushToast(
  message: string,
  kind: ToastKind = "info",
  durationMs = 3600,
): void {
  const id = nextToastId++;
  activeToasts.value.push({ id, kind, message });
  setTimeout(() => {
    dismissToast(id);
  }, durationMs);
}

export function dismissToast(id: number): void {
  const idx = activeToasts.value.findIndex((t) => t.id === id);
  if (idx >= 0) {
    activeToasts.value.splice(idx, 1);
  }
}

export const toast = {
  info: (msg: string) => pushToast(msg, "info"),
  success: (msg: string) => pushToast(msg, "success", 2800),
  error: (msg: string) => pushToast(msg, "error", 4800),
};
