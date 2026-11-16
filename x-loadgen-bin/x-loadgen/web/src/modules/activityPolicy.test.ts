import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  AUTO_FOCUS_GESTURE_WINDOW_MS,
  shouldAllowProgrammaticFocus,
  type FocusPolicyState,
} from "./activityPolicy.ts";

function base(over: Partial<FocusPolicyState> = {}): FocusPolicyState {
  return {
    documentHidden: false,
    visibilityState: "visible",
    hasFocus: true,
    blurredSinceGesture: false,
    gestureAgeMs: 16,
    maxGestureAgeMs: AUTO_FOCUS_GESTURE_WINDOW_MS,
    ...over,
  };
}

describe("shouldAllowProgrammaticFocus", () => {
  it("allows focus when the tab is visible, focused, and a recent gesture exists", () => {
    assert.equal(shouldAllowProgrammaticFocus(base()), true);
  });

  it("rejects a hidden document even if hasFocus is still true (macOS Spaces)", () => {
    assert.equal(
      shouldAllowProgrammaticFocus(base({ documentHidden: true })),
      false,
    );
    assert.equal(
      shouldAllowProgrammaticFocus(base({ visibilityState: "hidden" })),
      false,
    );
  });

  it("rejects when the window has already lost OS focus", () => {
    assert.equal(shouldAllowProgrammaticFocus(base({ hasFocus: false })), false);
  });

  it("rejects after blur/visibilitychange since the last user gesture", () => {
    assert.equal(
      shouldAllowProgrammaticFocus(base({ blurredSinceGesture: true })),
      false,
    );
  });

  it("rejects library auto-focus without a recent gesture", () => {
    assert.equal(
      shouldAllowProgrammaticFocus(base({ gestureAgeMs: null })),
      false,
    );
    assert.equal(
      shouldAllowProgrammaticFocus(
        base({ gestureAgeMs: AUTO_FOCUS_GESTURE_WINDOW_MS + 1 }),
      ),
      false,
    );
  });

  it("allows explicit focus() with no gesture-window limit while the window is frontmost", () => {
    assert.equal(
      shouldAllowProgrammaticFocus(
        base({ gestureAgeMs: null, maxGestureAgeMs: null }),
      ),
      true,
    );
  });
});
