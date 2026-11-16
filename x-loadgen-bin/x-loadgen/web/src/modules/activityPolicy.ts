/**
 * Pure policy for when programmatic focus is allowed.
 * Kept free of DOM / Vue so it can be unit-tested with Node's test runner.
 */

export const DEFERRED_FOCUS_DELAY_MS = 80;

/** Auto-focus after a library open/close event must follow a recent user gesture. */
export const AUTO_FOCUS_GESTURE_WINDOW_MS = 2_000;

export interface FocusPolicyState {
  /** `document.hidden` */
  documentHidden: boolean;
  /** `document.visibilityState` */
  visibilityState: string;
  /** `document.hasFocus()` */
  hasFocus: boolean;
  /** True once blur / hide happened after the last user gesture. */
  blurredSinceGesture: boolean;
  /** Milliseconds since the last trusted user gesture, or null if none. */
  gestureAgeMs: number | null;
  /** When set, a gesture older than this window is rejected. Null disables the check. */
  maxGestureAgeMs: number | null;
}

/**
 * Returns whether a programmatic `element.focus()` (or a component-library
 * auto-focus) should be allowed. Background tabs and other macOS Spaces must
 * never pass this check.
 */
export function shouldAllowProgrammaticFocus(state: FocusPolicyState): boolean {
  if (state.documentHidden) return false;
  if (state.visibilityState !== "visible") return false;
  if (!state.hasFocus) return false;
  if (state.blurredSinceGesture) return false;
  if (state.maxGestureAgeMs !== null) {
    if (state.gestureAgeMs === null) return false;
    if (state.gestureAgeMs > state.maxGestureAgeMs) return false;
    if (state.gestureAgeMs < 0) return false;
  }
  return true;
}
