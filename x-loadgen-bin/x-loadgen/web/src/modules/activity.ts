import { ref } from "vue";
import {
  AUTO_FOCUS_GESTURE_WINDOW_MS,
  DEFERRED_FOCUS_DELAY_MS,
  shouldAllowProgrammaticFocus,
} from "./activityPolicy";

export const isPageVisible = ref<boolean>(
  typeof document === "undefined" ? true : document.visibilityState === "visible",
);

export const isWindowFocused = ref<boolean>(
  typeof document === "undefined" ? true : document.hasFocus(),
);

let listenersAttached = false;
let lastGestureAt = 0;
let blurredSinceGesture = false;
let pendingFocusToken = 0;

function readDocumentState(): {
  documentHidden: boolean;
  visibilityState: string;
  hasFocus: boolean;
} {
  if (typeof document === "undefined") {
    return { documentHidden: false, visibilityState: "visible", hasFocus: true };
  }
  return {
    documentHidden: document.hidden,
    visibilityState: document.visibilityState,
    hasFocus: document.hasFocus(),
  };
}

function syncActivityState(): void {
  if (typeof document === "undefined") return;
  const snapshot = readDocumentState();
  isPageVisible.value =
    !snapshot.documentHidden && snapshot.visibilityState === "visible";
  isWindowFocused.value = isPageVisible.value && snapshot.hasFocus;
  if (!isWindowFocused.value) {
    blurredSinceGesture = true;
    pendingFocusToken += 1;
  }
}

function markUserGesture(): void {
  lastGestureAt = Date.now();
  blurredSinceGesture = false;
  syncActivityState();
}

function gestureHandler(): void {
  markUserGesture();
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
  // Capture trusted user gestures so library auto-focus can be tied to them.
  document.addEventListener("pointerdown", gestureHandler, {
    capture: true,
    passive: true,
  });
  document.addEventListener("keydown", gestureHandler, {
    capture: true,
    passive: true,
  });
}

/**
 * Returns true only when this browser tab is both visible and holds OS/window
 * focus. Used before explicit `.focus()` calls (rename, clipboard fallback,
 * keyboard tab cycling) and before handling global shortcuts.
 */
export function canProgrammaticallyFocus(): boolean {
  if (typeof document === "undefined") return false;
  const snapshot = readDocumentState();
  return shouldAllowProgrammaticFocus({
    ...snapshot,
    blurredSinceGesture: false,
    gestureAgeMs: null,
    maxGestureAgeMs: null,
  });
}

function firstFocusable(root: EventTarget | null): HTMLElement | null {
  if (!(root instanceof HTMLElement)) return null;
  const selector =
    'input:not([disabled]):not([type="hidden"]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])';
  const candidate = root.querySelector<HTMLElement>(selector);
  if (candidate) return candidate;
  if (root.tabIndex >= 0 || root.getAttribute("tabindex") !== null) {
    return root;
  }
  return root;
}

function allowLibraryAutoFocus(): boolean {
  if (typeof document === "undefined") return false;
  const snapshot = readDocumentState();
  const gestureAgeMs = lastGestureAt === 0 ? null : Date.now() - lastGestureAt;
  return shouldAllowProgrammaticFocus({
    ...snapshot,
    blurredSinceGesture,
    gestureAgeMs,
    maxGestureAgeMs: AUTO_FOCUS_GESTURE_WINDOW_MS,
  });
}

/**
 * Handler for Reka UI `open-auto-focus` / `close-auto-focus`.
 *
 * Always cancels the library's synchronous `.focus()` (that call is what
 * pulls a background window across macOS Spaces). If the window is still
 * frontmost after a short delay and a recent user gesture is on record,
 * focus is restored inside the dialog/menu.
 */
export function guardAutoFocusEvent(event: Event): void {
  event.preventDefault();
  if (typeof window === "undefined") return;
  const root = event.currentTarget ?? event.target;
  const token = ++pendingFocusToken;
  window.setTimeout(() => {
    if (token !== pendingFocusToken) return;
    if (!allowLibraryAutoFocus()) return;
    const target = firstFocusable(root);
    if (target && allowLibraryAutoFocus()) {
      target.focus({ preventScroll: true });
    }
  }, DEFERRED_FOCUS_DELAY_MS);
}
