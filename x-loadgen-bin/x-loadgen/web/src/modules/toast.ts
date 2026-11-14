import { ref } from "vue";

export type ToastKind = "info" | "success" | "error";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
  /** How many times the same message fired while it was on screen. */
  count: number;
}

/** At most this many toasts are visible; the oldest is dropped first. */
export const MAX_TOASTS = 4;

export const activeToasts = ref<ToastItem[]>([]);
let nextToastId = 1;
const timers = new Map<number, number>();

function schedule(id: number, durationMs: number): void {
  const prev = timers.get(id);
  if (prev !== undefined) window.clearTimeout(prev);
  timers.set(
    id,
    window.setTimeout(() => dismissToast(id), durationMs),
  );
}

/**
 * Show a toast. A message identical to one already on screen (same kind and
 * text) is merged into it — its counter goes up and its timer restarts —
 * so a repeating failure shows one toast instead of a growing stack.
 */
export function pushToast(
  message: string,
  kind: ToastKind = "info",
  durationMs = 3600,
): void {
  const existing = activeToasts.value.find(
    (t) => t.kind === kind && t.message === message,
  );
  if (existing) {
    existing.count += 1;
    schedule(existing.id, durationMs);
    return;
  }
  const id = nextToastId++;
  activeToasts.value.push({ id, kind, message, count: 1 });
  schedule(id, durationMs);
  while (activeToasts.value.length > MAX_TOASTS) {
    const oldest = activeToasts.value[0];
    if (!oldest) break;
    dismissToast(oldest.id);
  }
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
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
