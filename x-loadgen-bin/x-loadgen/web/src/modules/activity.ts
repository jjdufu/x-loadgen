import { ref } from "vue";

export const isPageVisible = ref<boolean>(
  typeof document === "undefined" ? true : document.visibilityState === "visible",
);

export const isWindowFocused = ref<boolean>(
  typeof document === "undefined" ? true : document.hasFocus(),
);

let listenersAttached = false;

function syncActivityState(): void {
  if (typeof document === "undefined") return;
  isPageVisible.value = document.visibilityState === "visible";
  isWindowFocused.value =
    document.visibilityState === "visible" && document.hasFocus();
}

export function initActivityMonitor(): void {
  if (listenersAttached || typeof window === "undefined") return;
  listenersAttached = true;
  syncActivityState();
  document.addEventListener("visibilitychange", syncActivityState, {
    passive: true,
  });
  window.addEventListener("focus", syncActivityState, { passive: true });
  window.addEventListener("blur", syncActivityState, { passive: true });
  window.addEventListener("pageshow", syncActivityState, { passive: true });
  window.addEventListener("pagehide", syncActivityState, { passive: true });
}

/**
 * Returns true only when this browser tab is both visible and holds OS/window focus.
 * Prevents background tabs on another macOS Space / virtual desktop from stealing focus.
 */
export function canProgrammaticallyFocus(): boolean {
  if (typeof document === "undefined") return false;
  return document.visibilityState === "visible" && document.hasFocus();
}

export function guardAutoFocusEvent(event: Event): void {
  if (!canProgrammaticallyFocus()) {
    event.preventDefault();
  }
}
