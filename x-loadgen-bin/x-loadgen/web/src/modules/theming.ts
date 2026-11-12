import { computed, ref, watch } from "vue";

export type HoppBgColor = "system" | "light" | "dark" | "black";

export type HoppAccentColor =
  | "green"
  | "teal"
  | "blue"
  | "indigo"
  | "purple"
  | "yellow"
  | "orange"
  | "red"
  | "pink";

export const HOPP_BG_COLORS: readonly {
  value: HoppBgColor;
  label: string;
  desc: string;
}[] = [
  { value: "system", label: "跟随系统 (System)", desc: "自动跟随操作系统深浅色" },
  { value: "light", label: "浅色 (Light)", desc: "高对比明亮日间主题" },
  { value: "dark", label: "深灰 (Dark)", desc: "Hoppscotch 默认经典深色" },
  { value: "black", label: "纯黑 (Black / OLED)", desc: "OLED 纯黑深邃背景" },
];

export const HOPP_ACCENT_COLORS: readonly {
  value: HoppAccentColor;
  label: string;
  hex: string;
}[] = [
  { value: "green", label: "Green", hex: "#10b981" },
  { value: "teal", label: "Teal", hex: "#14b8a6" },
  { value: "blue", label: "Blue", hex: "#3b82f6" },
  { value: "indigo", label: "Indigo", hex: "#6366f1" },
  { value: "purple", label: "Purple", hex: "#a855f7" },
  { value: "yellow", label: "Yellow", hex: "#eab308" },
  { value: "orange", label: "Orange", hex: "#f97316" },
  { value: "red", label: "Red", hex: "#ef4444" },
  { value: "pink", label: "Pink", hex: "#ec4899" },
];

const BG_STORAGE_KEY = "x-loadgen.theme.bg";
const ACCENT_STORAGE_KEY = "x-loadgen.theme.accent";

function readInitialBg(): HoppBgColor {
  const raw = localStorage.getItem(BG_STORAGE_KEY);
  if (raw === "system" || raw === "light" || raw === "dark" || raw === "black") {
    return raw;
  }
  return "dark";
}

function readInitialAccent(): HoppAccentColor {
  const raw = localStorage.getItem(ACCENT_STORAGE_KEY);
  if (HOPP_ACCENT_COLORS.some((c) => c.value === raw)) {
    return raw as HoppAccentColor;
  }
  return "green";
}

export const bgColor = ref<HoppBgColor>(readInitialBg());
export const accentColor = ref<HoppAccentColor>(readInitialAccent());
const systemPrefersDark = ref<boolean>(
  typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches,
);

export const effectiveColorMode = computed<"light" | "dark" | "black">(() => {
  if (bgColor.value === "system") {
    return systemPrefersDark.value ? "dark" : "light";
  }
  return bgColor.value;
});

function applyBgClass(mode: "light" | "dark" | "black"): void {
  const root = document.documentElement;
  root.classList.remove("light", "dark", "black");
  root.classList.add(mode);
}

function applyAccentAttr(accent: HoppAccentColor): void {
  document.documentElement.setAttribute("data-accent", accent);
}

let initialized = false;

export function initHoppscotchTheming(): void {
  if (initialized) return;
  initialized = true;

  if (typeof window !== "undefined") {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    systemPrefersDark.value = mq.matches;
    mq.addEventListener("change", (ev: MediaQueryListEvent) => {
      systemPrefersDark.value = ev.matches;
    });
  }

  watch(
    bgColor,
    (next) => {
      localStorage.setItem(BG_STORAGE_KEY, next);
    },
    { immediate: true },
  );

  watch(
    effectiveColorMode,
    (mode) => {
      applyBgClass(mode);
    },
    { immediate: true },
  );

  watch(
    accentColor,
    (next) => {
      localStorage.setItem(ACCENT_STORAGE_KEY, next);
      applyAccentAttr(next);
    },
    { immediate: true },
  );
}

export function cycleColorMode(): void {
  const order: HoppBgColor[] = ["dark", "black", "light", "system"];
  const idx = order.indexOf(bgColor.value);
  bgColor.value = order[(idx + 1) % order.length] ?? "dark";
}
