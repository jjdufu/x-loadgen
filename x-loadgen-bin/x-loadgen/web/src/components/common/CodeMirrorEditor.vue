<template>
  <div class="flex flex-col overflow-hidden rounded-lg border border-border bg-background">
    <div
      v-if="showToolbar"
      class="flex items-center justify-between border-b border-border bg-muted/35 px-3 py-1.5 text-xs text-muted-foreground"
    >
      <span class="font-mono text-[11px] font-medium uppercase tracking-wider">
        {{ languageLabel }}
      </span>
      <div class="flex items-center gap-1">
        <UiButton
          v-if="!readonly && language === 'json'"
          variant="ghost"
          size="xs"
          title="格式化 JSON"
          @click="prettifyJson"
        >
          <WandSparkles class="h-3.5 w-3.5" />
          <span>格式化</span>
        </UiButton>
        <UiButton
          variant="ghost"
          size="xs"
          title="复制到剪贴板"
          @click="copyContent"
        >
          <Copy class="h-3.5 w-3.5" />
          <span>复制</span>
        </UiButton>
      </div>
    </div>
    <div ref="hostEl" class="cm-geist-host" :style="{ minHeight, maxHeight }" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { EditorState, Compartment, type Extension } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  placeholder as cmPlaceholder,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { json } from "@codemirror/lang-json";
import {
  HighlightStyle,
  syntaxHighlighting,
  bracketMatching,
} from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { Copy, WandSparkles } from "lucide-vue-next";
import { effectiveTheme, type ThemeMode } from "../../modules/theme";
import { toast } from "../../modules/toast";
import { copyText } from "../../utils/clipboard";
import UiButton from "../ui/UiButton.vue";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    language?: "json" | "text";
    readonly?: boolean;
    placeholder?: string;
    minHeight?: string;
    maxHeight?: string;
    showToolbar?: boolean;
  }>(),
  {
    language: "json",
    readonly: false,
    placeholder: "",
    minHeight: "160px",
    maxHeight: "480px",
    showToolbar: true,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const hostEl = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;
const readOnlyCompartment = new Compartment();
const langCompartment = new Compartment();
const themeCompartment = new Compartment();

const languageLabel = computed(() =>
  props.language === "json" ? "JSON" : "RAW TEXT",
);

function buildGeistEditorTheme(mode: ThemeMode): Extension {
  const isDark = mode === "black";
  const uiTheme = EditorView.theme(
    {
      "&": {
        backgroundColor: isDark ? "#09090b" : "#ffffff",
        color: isDark ? "#fafafa" : "#09090b",
        fontSize: "13px",
        fontFamily:
          '"Geist Mono Variable", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      },
      ".cm-content": {
        caretColor: isDark ? "#fafafa" : "#09090b",
        padding: "10px 0",
      },
      ".cm-line": {
        padding: "0 12px",
      },
      ".cm-gutters": {
        backgroundColor: isDark ? "#0c0c0f" : "#fafafa",
        color: isDark ? "#71717a" : "#a1a1aa",
        borderRight: isDark ? "1px solid #27272a" : "1px solid #e4e4e7",
        minWidth: "40px",
      },
      ".cm-activeLine": {
        backgroundColor: isDark ? "rgba(255,255,255,0.03)" : "rgba(9,9,11,0.025)",
      },
      ".cm-activeLineGutter": {
        backgroundColor: isDark ? "rgba(255,255,255,0.04)" : "rgba(9,9,11,0.04)",
        color: isDark ? "#d4d4d8" : "#52525b",
      },
      "&.cm-focused": {
        outline: "none",
      },
      "&.cm-focused .cm-cursor": {
        borderLeftColor: isDark ? "#fafafa" : "#09090b",
      },
      ".cm-scroller": {
        fontFamily:
          '"Geist Mono Variable", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        lineHeight: "1.6",
      },
    },
    { dark: isDark },
  );

  const highlight = HighlightStyle.define(
    isDark
      ? [
          { tag: tags.propertyName, color: "#79c0ff" },
          { tag: tags.string, color: "#7ee787" },
          { tag: tags.number, color: "#ffa657" },
          { tag: tags.bool, color: "#d2a8ff" },
          { tag: tags.null, color: "#ff7b72" },
          { tag: tags.punctuation, color: "#a1a1aa" },
        ]
      : [
          { tag: tags.propertyName, color: "#0550ae" },
          { tag: tags.string, color: "#116329" },
          { tag: tags.number, color: "#953800" },
          { tag: tags.bool, color: "#8250df" },
          { tag: tags.null, color: "#cf222e" },
          { tag: tags.punctuation, color: "#71717a" },
        ],
  );

  return [uiTheme, syntaxHighlighting(highlight)];
}

onMounted(() => {
  if (!hostEl.value) return;
  const state = EditorState.create({
    doc: props.modelValue ?? "",
    extensions: [
      lineNumbers(),
      history(),
      bracketMatching(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      themeCompartment.of(buildGeistEditorTheme(effectiveTheme.value)),
      EditorView.lineWrapping,
      ...(props.placeholder ? [cmPlaceholder(props.placeholder)] : []),
      readOnlyCompartment.of(EditorState.readOnly.of(props.readonly)),
      langCompartment.of(props.language === "json" ? json() : []),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          const text = update.state.doc.toString();
          if (text !== props.modelValue) {
            emit("update:modelValue", text);
          }
        }
      }),
    ],
  });
  view = new EditorView({ state, parent: hostEl.value });
});

watch(effectiveTheme, (mode) => {
  view?.dispatch({
    effects: themeCompartment.reconfigure(buildGeistEditorTheme(mode)),
  });
});

watch(
  () => props.modelValue,
  (nextVal) => {
    if (!view) return;
    const current = view.state.doc.toString();
    if ((nextVal ?? "") !== current) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: nextVal ?? "" },
      });
    }
  },
);

watch(
  () => props.readonly,
  (ro) => {
    view?.dispatch({
      effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(ro)),
    });
  },
);

watch(
  () => props.language,
  (lang) => {
    view?.dispatch({
      effects: langCompartment.reconfigure(lang === "json" ? json() : []),
    });
  },
);

function prettifyJson(): void {
  const raw = props.modelValue.trim();
  if (!raw) return;
  try {
    const formatted = JSON.stringify(JSON.parse(raw), null, 2);
    emit("update:modelValue", formatted);
    toast.success("JSON 已格式化");
  } catch (err) {
    toast.error(
      `JSON 解析失败：${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

async function copyContent(): Promise<void> {
  const ok = await copyText(props.modelValue ?? "");
  if (ok) {
    toast.success("已复制到剪贴板");
  } else {
    toast.error("复制失败");
  }
}

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});
</script>

<style scoped>
.cm-geist-host :deep(.cm-editor) {
  height: 100%;
  max-height: inherit;
}
.cm-geist-host :deep(.cm-scroller) {
  overflow: auto;
}
</style>
