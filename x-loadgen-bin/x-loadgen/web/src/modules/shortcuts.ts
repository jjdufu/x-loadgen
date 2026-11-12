import { ref } from "vue";

export type ShortcutId =
  | "sendRequest"
  | "saveRequest"
  | "newRequest"
  | "switchProtocol"
  | "toggleResponse";

export interface ShortcutDefinition {
  id: ShortcutId;
  label: string;
  description: string;
  defaultBinding: string;
}

export const SHORTCUT_DEFINITIONS: ShortcutDefinition[] = [
  {
    id: "sendRequest",
    label: "发送请求",
    description: "在工作台发送当前标签页请求",
    defaultBinding: "Mod+Enter",
  },
  {
    id: "saveRequest",
    label: "保存请求",
    description: "保存当前请求草稿到集合",
    defaultBinding: "Mod+S",
  },
  {
    id: "newRequest",
    label: "新建请求",
    description: "在工作台新建空白请求标签页",
    defaultBinding: "Alt+N",
  },
  {
    id: "switchProtocol",
    label: "切换协议模式",
    description: "在 HTTP 通用与 Mazbri 网关之间切换",
    defaultBinding: "Alt+M",
  },
  {
    id: "toggleResponse",
    label: "展开 / 收起响应栏",
    description: "切换工作台响应面板显示状态",
    defaultBinding: "Alt+R",
  },
];

const STORAGE_KEY = "x-loadgen.shortcuts.v1";

function getDefaultBindings(): Record<ShortcutId, string> {
  return {
    sendRequest: "Mod+Enter",
    saveRequest: "Mod+S",
    newRequest: "Alt+N",
    switchProtocol: "Alt+M",
    toggleResponse: "Alt+R",
  };
}

function loadBindings(): Record<ShortcutId, string> {
  const defaults = getDefaultBindings();
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<Record<ShortcutId, string>>;
    if (!parsed || typeof parsed !== "object") return defaults;
    const merged = { ...defaults };
    for (const def of SHORTCUT_DEFINITIONS) {
      if (typeof parsed[def.id] === "string") {
        merged[def.id] = parsed[def.id]!;
      }
    }
    return merged;
  } catch {
    return defaults;
  }
}

function persistBindings(map: Record<ShortcutId, string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
  } catch {
    // Ignore storage errors
  }
}

export const shortcutBindings = ref<Record<ShortcutId, string>>(loadBindings());
export const recordingShortcutId = ref<ShortcutId | null>(null);

const isMacPlatform =
  typeof navigator !== "undefined" &&
  /Mac|iPod|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);

function normalizeEventKey(rawKey: string): string {
  if (rawKey === " ") return "Space";
  if (rawKey.length === 1) return rawKey.toUpperCase();
  return rawKey;
}

export function formatBindingDisplay(binding: string): string {
  if (!binding) return "未绑定";
  return binding
    .split("+")
    .map((part) => {
      if (part === "Mod") return isMacPlatform ? "⌘" : "Ctrl";
      if (part === "Alt") return isMacPlatform ? "⌥" : "Alt";
      return part;
    })
    .join(" + ");
}

export function captureBindingFromEvent(e: KeyboardEvent): {
  binding: string | null;
  reason?: "modifier_only" | "needs_modifier";
} {
  if (
    e.key === "Control" ||
    e.key === "Meta" ||
    e.key === "Alt" ||
    e.key === "Shift"
  ) {
    return { binding: null, reason: "modifier_only" };
  }

  const key = normalizeEventKey(e.key);
  const isFunctionKey = /^F([1-9]|1[0-2])$/i.test(key);
  const hasPrimaryModifier = e.ctrlKey || e.metaKey || e.altKey;

  if (!hasPrimaryModifier && !isFunctionKey) {
    return { binding: null, reason: "needs_modifier" };
  }

  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Mod");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(isFunctionKey ? key.toUpperCase() : key);
  return { binding: parts.join("+") };
}

export function matchShortcut(e: KeyboardEvent, binding: string): boolean {
  if (!binding) return false;
  const parts = binding.split("+");
  const targetKey = parts[parts.length - 1];
  if (!targetKey) return false;

  const wantMod = parts.includes("Mod");
  const wantAlt = parts.includes("Alt");
  const wantShift = parts.includes("Shift");

  if ((e.ctrlKey || e.metaKey) !== wantMod) return false;
  if (e.altKey !== wantAlt) return false;
  if (e.shiftKey !== wantShift) return false;

  return normalizeEventKey(e.key).toLowerCase() === targetKey.toLowerCase();
}

export function setShortcutBinding(
  id: ShortcutId,
  binding: string,
): { ok: boolean; conflictLabel?: string } {
  if (binding) {
    for (const def of SHORTCUT_DEFINITIONS) {
      if (def.id !== id && shortcutBindings.value[def.id] === binding) {
        return { ok: false, conflictLabel: def.label };
      }
    }
  }
  const next = {
    ...shortcutBindings.value,
    [id]: binding,
  };
  shortcutBindings.value = next;
  persistBindings(next);
  return { ok: true };
}

export function clearShortcutBinding(id: ShortcutId): void {
  setShortcutBinding(id, "");
}

export function resetAllShortcuts(): void {
  const defaults = getDefaultBindings();
  shortcutBindings.value = defaults;
  persistBindings(defaults);
}
