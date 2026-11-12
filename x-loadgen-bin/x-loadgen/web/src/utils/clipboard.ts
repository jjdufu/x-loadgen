import { canProgrammaticallyFocus } from "../modules/activity";

/** Copy `text` to the clipboard. Uses the async API when available, otherwise a textarea fallback for HTTP origins. */
export async function copyText(text: string): Promise<boolean> {
  if (
    typeof navigator !== "undefined" &&
    navigator.clipboard &&
    typeof window !== "undefined" &&
    window.isSecureContext
  ) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Fall through to the execCommand path.
    }
  }
  if (typeof document === "undefined") return false;
  try {
    const prevFocused =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.setAttribute("aria-hidden", "true");
    area.tabIndex = -1;
    area.style.position = "fixed";
    area.style.left = "-9999px";
    area.style.top = "0";
    area.style.opacity = "0";
    document.body.appendChild(area);
    if (canProgrammaticallyFocus()) {
      area.focus({ preventScroll: true });
    }
    area.select();
    area.setSelectionRange(0, text.length);
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    if (prevFocused && canProgrammaticallyFocus()) {
      prevFocused.focus({ preventScroll: true });
    }
    return ok;
  } catch {
    return false;
  }
}
