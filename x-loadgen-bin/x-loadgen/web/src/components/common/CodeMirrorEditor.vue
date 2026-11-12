<template>
  <div class="flex flex-col border border-dividerLight rounded bg-primary overflow-hidden">
    <div
      v-if="showToolbar"
      class="flex items-center justify-between px-3 py-1.5 border-b border-dividerLight bg-primaryLight text-tiny text-secondary"
    >
      <span class="font-mono uppercase tracking-wider">{{ languageLabel }}</span>
      <div class="flex items-center space-x-1">
        <HoppButtonSecondary
          v-if="!readonly && language === 'json'"
          v-tippy="'格式化 JSON'"
          :icon="WandSparkles"
          label="美化"
          @click="prettifyJson"
        />
        <HoppButtonSecondary
          v-tippy="'复制内容'"
          :icon="Copy"
          label="复制"
          @click="copyContent"
        />
      </div>
    </div>
    <div ref="hostEl" class="cm-hopp-host text-body" :style="{ minHeight, maxHeight }" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { EditorState, Compartment } from "@codemirror/state";
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
import { HoppButtonSecondary } from "@hoppscotch/ui";
import { Copy, WandSparkles } from "lucide-vue-next";
import { toast } from "../../modules/toast";

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

const languageLabel = computed(() =>
  props.language === "json" ? "JSON" : "RAW TEXT",
);

const hoppEditorTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--primary-color)",
    color: "var(--secondary-dark-color)",
    fontSize: "12px",
    fontFamily: "var(--font-mono, monospace)",
  },
  ".cm-content": {
    caretColor: "var(--accent-color)",
    padding: "8px 0",
  },
  ".cm-gutters": {
    backgroundColor: "var(--primary-color)",
    color: "var(--secondary-light-color)",
    borderRight: "1px solid var(--divider-light-color)",
    minWidth: "36px",
  },
  "&.cm-focused .cm-cursor": {
    borderLeftColor: "var(--accent-color)",
  },
  ".cm-scroller": {
    fontFamily: "var(--font-mono, monospace)",
    lineHeight: "1.55",
  },
});

const hoppHighlight = HighlightStyle.define([
  { tag: tags.propertyName, color: "var(--editor-type-key, #38bdf8)" },
  { tag: tags.string, color: "var(--editor-type-string, #10b981)" },
  { tag: tags.number, color: "var(--editor-type-number, #f59e0b)" },
  { tag: tags.bool, color: "var(--editor-type-boolean, #a855f7)" },
  { tag: tags.null, color: "var(--editor-type-null, #ef4444)" },
  { tag: tags.punctuation, color: "var(--secondary-color)" },
]);

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
      hoppEditorTheme,
      syntaxHighlighting(hoppHighlight),
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
  try {
    await navigator.clipboard.writeText(props.modelValue ?? "");
    toast.success("已复制到剪贴板");
  } catch {
    toast.error("复制失败");
  }
}

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});
</script>

<style scoped>
.cm-hopp-host :deep(.cm-editor) {
  height: 100%;
  max-height: inherit;
}
.cm-hopp-host :deep(.cm-scroller) {
  overflow: auto;
}
</style>
