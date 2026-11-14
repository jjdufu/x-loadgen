import { ref } from "vue";

/** Below Tailwind `md` (768px): phone layout, navigation in a drawer. */
const MOBILE_QUERY = "(max-width: 767.98px)";
/** Below Tailwind `lg` (1024px): no room for docked side panels. */
const COMPACT_QUERY = "(max-width: 1023.98px)";

function trackMedia(query: string) {
  const mql = window.matchMedia(query);
  const state = ref(mql.matches);
  mql.addEventListener("change", (e) => {
    state.value = e.matches;
  });
  return state;
}

/** Phone-width viewport. */
export const isMobile = trackMedia(MOBILE_QUERY);
/** Phone or tablet width: side panels become overlays. */
export const isCompact = trackMedia(COMPACT_QUERY);

/** Mobile navigation drawer state (header hamburger / nav overlay). */
export const mobileNavOpen = ref(false);

/**
 * Workspace right panel as an overlay drawer on compact viewports. Kept apart
 * from the docked-panel preference so a desktop "open" setting doesn't cover
 * the editor on a phone.
 */
export const compactRightPanelOpen = ref(false);

// Escape closes whichever drawer is open (dialogs handle their own Escape).
window.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (document.querySelector('[role="dialog"]')) return;
  mobileNavOpen.value = false;
  compactRightPanelOpen.value = false;
});
