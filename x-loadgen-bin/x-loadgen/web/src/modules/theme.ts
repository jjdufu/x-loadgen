import { computed, ref, watch } from "vue";

export type ThemeMode = "light" | "black";

const STORAGE_KEY = "x-loadgen.theme.v3";

function readSavedOverride(): ThemeMode | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "black") {
      return raw;
    }
    return null;
  } catch {
    return null;
  }
}

function detectSystemPrefersBlack(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) {
    return false;
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export const userThemeOverride = ref<ThemeMode | null>(readSavedOverride());
export const systemPrefersBlack = ref<boolean>(detectSystemPrefersBlack());

export const effectiveTheme = computed<ThemeMode>(() => {
  if (userThemeOverride.value) {
    return userThemeOverride.value;
  }
  return systemPrefersBlack.value ? "black" : "light";
});

export const isFollowingSystem = computed<boolean>(
  () => userThemeOverride.value === null,
);

function applyDomTheme(mode: ThemeMode): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (mode === "black") {
    root.classList.add("dark");
    root.style.colorScheme = "dark";
  } else {
    root.classList.remove("dark");
    root.style.colorScheme = "light";
  }
}

export function setThemeMode(mode: ThemeMode): void {
  userThemeOverride.value = mode;
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // Ignore storage quota errors
  }
  applyDomTheme(effectiveTheme.value);
}

export function toggleTheme(): void {
  const next: ThemeMode = effectiveTheme.value === "black" ? "light" : "black";
  setThemeMode(next);
}

export function resetToSystemTheme(): void {
  userThemeOverride.value = null;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage errors
  }
  applyDomTheme(effectiveTheme.value);
}

let initialized = false;

export function initTheme(): void {
  if (initialized) return;
  initialized = true;

  if (typeof window !== "undefined" && window.matchMedia) {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    systemPrefersBlack.value = mql.matches;
    const handler = (ev: MediaQueryListEvent) => {
      systemPrefersBlack.value = ev.matches;
    };
    mql.addEventListener("change", handler);
  }

  applyDomTheme(effectiveTheme.value);

  watch(effectiveTheme, (mode) => {
    applyDomTheme(mode);
  });
}
