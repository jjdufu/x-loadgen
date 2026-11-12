<template>
  <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background">
    <!-- Loading State -->
    <div
      v-if="activeTab.sending"
      class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-muted-foreground"
    >
      <UiButton
        variant="ghost"
        size="icon-xs"
        title="收起响应栏"
        aria-label="收起响应栏"
        class="absolute right-3 top-2.5"
        @click="responsePaneOpen = false"
      >
        <X class="h-3.5 w-3.5" />
      </UiButton>
      <Loader2 class="h-6 w-6 animate-spin text-foreground" />
      <span class="text-sm">正在发送请求...</span>
    </div>

    <!-- Empty Initial State -->
    <div
      v-else-if="!resp"
      class="flex flex-1 flex-col items-center justify-center gap-2.5 p-8 text-center select-none"
    >
      <UiButton
        variant="ghost"
        size="icon-xs"
        title="收起响应栏"
        aria-label="收起响应栏"
        class="absolute right-3 top-2.5"
        @click="responsePaneOpen = false"
      >
        <X class="h-3.5 w-3.5" />
      </UiButton>
      <div
        class="flex h-10 w-10 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground"
      >
        <Send class="h-4 w-4" />
      </div>
      <p class="max-w-sm text-xs text-muted-foreground">
        点击「发送」调试当前请求，响应结果将在此展示
      </p>
    </div>

    <!-- Active Probe Response -->
    <template v-else>
      <!-- Shadcn Response Meta Bar -->
      <div
        class="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-surface/50 px-4 py-2 text-xs shrink-0"
      >
        <div class="flex flex-wrap items-center gap-3">
          <span
            class="inline-flex items-center rounded-md border px-2 py-0.5 font-mono text-xs font-semibold tabular-nums"
            :class="statusCategoryClass(resp.http_status, resp.transport_ok)"
          >
            {{
              resp.http_status
                ? `${resp.http_status} ${resp.status_text ?? ""}`.trim()
                : resp.error_class || "异常"
            }}
          </span>

          <div class="flex items-center gap-1.5 text-muted-foreground">
            <span>耗时</span>
            <span class="font-mono font-medium text-foreground tabular-nums">
              {{ formatLatencyUs(resp.latency_us) }}
            </span>
          </div>

          <span class="text-border">·</span>

          <div class="flex items-center gap-1.5 text-muted-foreground">
            <span>大小</span>
            <span class="font-mono font-medium text-foreground tabular-nums">
              {{ formatBytes(resp.response_bytes) }}
            </span>
          </div>

          <UiBadge :variant="resp.passed_assertions ? 'success' : 'destructive'">
            {{ resp.passed_assertions ? "断言通过" : "断言未通过" }}
          </UiBadge>
        </div>

        <div class="flex items-center gap-1">
          <UiButton
            variant="ghost"
            size="icon-xs"
            title="复制响应正文"
            aria-label="复制响应正文"
            @click="copyResponse"
          >
            <Copy class="h-3.5 w-3.5" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon-xs"
            title="下载响应内容"
            aria-label="下载响应内容"
            @click="downloadResponse"
          >
            <Download class="h-3.5 w-3.5" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon-xs"
            title="收起响应栏"
            aria-label="收起响应栏"
            @click="responsePaneOpen = false"
          >
            <X class="h-3.5 w-3.5" />
          </UiButton>
        </div>
      </div>

      <!-- Unresolved <<var>> Warning Banner -->
      <div
        v-if="resp.request_preview.unresolved_variables.length > 0"
        class="flex items-center gap-2 border-b border-amber-500/25 bg-amber-500/10 px-4 py-2 text-xs text-amber-700 dark:text-amber-400 shrink-0"
      >
        <AlertTriangle class="h-4 w-4 shrink-0" />
        <span>
          检测到未解析的模板变量：
          <code class="font-mono font-semibold">
            {{ resp.request_preview.unresolved_variables.join(", ") }}
          </code>
          （请检查右上角是否已选择对应运行环境）
        </span>
      </div>

      <!-- Error Banner -->
      <div
        v-if="resp.error"
        class="border-b border-rose-500/25 bg-rose-500/10 px-4 py-2 font-mono text-xs text-rose-700 dark:text-rose-400 shrink-0"
      >
        [{{ resp.error_class || "transport_error" }}] {{ resp.error }}
      </div>

      <!-- Response Underline Tabs -->
      <div
        class="flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-b border-border bg-background px-4 select-none"
        role="tablist"
      >
        <button
          v-for="tab in responseTabs"
          :key="tab.id"
          type="button"
          role="tab"
          :aria-selected="activeTab.responseSubTab === tab.id"
          class="relative inline-flex h-full cursor-pointer items-center gap-1.5 px-3 text-xs font-medium whitespace-nowrap transition-colors"
          :class="
            activeTab.responseSubTab === tab.id
              ? 'text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          "
          @click="activeTab.responseSubTab = tab.id"
        >
          <span>{{ tab.label }}</span>
          <span
            v-if="tab.count"
            class="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-foreground tabular-nums"
          >
            {{ tab.count }}
          </span>
          <span
            v-else-if="tab.dot"
            class="h-1.5 w-1.5 rounded-full"
            :class="tab.errorDot ? 'bg-rose-500' : 'bg-foreground/75'"
          />
          <span
            v-if="activeTab.responseSubTab === tab.id"
            class="absolute inset-x-2 bottom-0 h-[2px] rounded-full bg-foreground"
          />
        </button>
      </div>

      <!-- Response Tab Content -->
      <div class="min-h-0 flex-1 overflow-y-auto">
        <div v-if="activeTab.responseSubTab === 'json'" class="p-4">
          <CodeMirrorEditor
            :model-value="prettyBodyText"
            :language="resp.body_json !== undefined && resp.body_json !== null ? 'json' : 'text'"
            readonly
            min-height="200px"
            max-height="540px"
          />
        </div>

        <div v-else-if="activeTab.responseSubTab === 'raw'" class="p-4">
          <pre
            class="max-h-[540px] overflow-auto rounded-lg border border-border bg-muted/35 p-4 font-mono text-[13px] leading-relaxed text-foreground whitespace-pre-wrap break-all select-text"
          >{{ resp.body_text || "(空响应体)" }}</pre>
        </div>

        <div
          v-else-if="activeTab.responseSubTab === 'headers'"
          class="divide-y divide-border border-b border-border font-mono text-xs select-text"
        >
          <div
            v-for="([k, v], idx) in resp.headers"
            :key="idx"
            class="grid grid-cols-3 px-4 py-2.5 hover:bg-muted/35"
          >
            <span class="truncate pr-3 text-muted-foreground">{{ k }}</span>
            <span class="col-span-2 break-all text-foreground">{{ v }}</span>
          </div>
        </div>

        <div v-else-if="activeTab.responseSubTab === 'assertions'" class="p-4 space-y-3">
          <div
            class="rounded-lg border px-4 py-2.5 text-xs font-semibold"
            :class="
              resp.passed_assertions
                ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400'
                : 'border-rose-500/30 bg-rose-500/10 text-rose-700 dark:text-rose-400'
            "
          >
            传输与业务综合判定：{{ resp.passed_assertions ? "全部通过" : "未通过" }}
          </div>

          <div
            v-for="(item, idx) in resp.assertions"
            :key="idx"
            class="rounded-lg border border-border bg-card p-3.5 font-mono text-xs space-y-1.5"
          >
            <div class="flex items-center gap-2">
              <span
                class="font-semibold"
                :class="
                  item.passed
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-rose-600 dark:text-rose-400'
                "
              >
                {{ item.passed ? "通过" : "未通过" }}
              </span>
              <span class="font-semibold text-foreground">{{ item.path }}</span>
            </div>
            <div v-if="item.expected !== undefined" class="text-muted-foreground">
              期望值: <span class="text-foreground">{{ JSON.stringify(item.expected) }}</span>
            </div>
            <div class="text-muted-foreground">
              实际值: <span class="text-foreground">{{ JSON.stringify(item.actual) }}</span>
            </div>
            <div v-if="item.message" class="text-rose-600 dark:text-rose-400">
              {{ item.message }}
            </div>
          </div>
        </div>

        <div
          v-else-if="activeTab.responseSubTab === 'preview'"
          class="p-4 space-y-3 font-mono text-xs select-text"
        >
          <div class="rounded-lg border border-border bg-muted/35 p-3">
            <span class="mr-2 font-semibold" :class="methodColorClass(resp.request_preview.method)">
              {{ resp.request_preview.method }}
            </span>
            <span class="break-all text-foreground">{{ resp.request_preview.url }}</span>
          </div>

          <div
            v-if="resp.request_preview.headers.length"
            class="divide-y divide-border overflow-hidden rounded-lg border border-border"
          >
            <div
              v-for="([hk, hv], idx) in resp.request_preview.headers"
              :key="idx"
              class="grid grid-cols-3 px-3.5 py-2"
            >
              <span class="text-muted-foreground">{{ hk }}</span>
              <span class="col-span-2 break-all text-foreground">{{ hv }}</span>
            </div>
          </div>

          <div v-if="resp.request_preview.body_preview" class="space-y-1.5">
            <div class="font-sans text-xs font-medium text-muted-foreground">发出载荷预览</div>
            <pre
              class="rounded-lg border border-border bg-muted/35 p-3.5 whitespace-pre-wrap break-all text-foreground"
            >{{ resp.request_preview.body_preview }}</pre>
          </div>
        </div>

        <!-- Mazbri Cryptographic Wire Inspector -->
        <div
          v-else-if="activeTab.responseSubTab === 'mazbri_wire' && resp.mazbri_wire"
          class="p-4 space-y-4 text-xs select-text"
        >
          <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
            <div class="rounded-lg border border-border bg-card p-3">
              <div class="text-muted-foreground">命中租户</div>
              <div class="mt-1 font-mono text-sm font-semibold text-foreground">
                {{ resp.mazbri_wire.tenant_name }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-card p-3">
              <div class="text-muted-foreground">加密模式 / 套件</div>
              <div class="mt-1 font-mono text-sm font-semibold text-foreground truncate">
                {{ resp.mazbri_wire.encryption_mode }}
                {{ resp.mazbri_wire.suite ? `· ${resp.mazbri_wire.suite}` : "" }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-card p-3">
              <div class="text-muted-foreground">/auth/token 换票耗时</div>
              <div class="mt-1 font-mono text-sm font-semibold text-foreground tabular-nums">
                {{ formatLatencyUs(resp.mazbri_wire.token_exchange_us) }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-card p-3">
              <div class="text-muted-foreground">请求绑定签名</div>
              <div class="mt-1 font-mono text-sm font-semibold text-foreground truncate">
                {{ resp.mazbri_wire.binding_alg || "未启用" }}
                {{ resp.mazbri_wire.binding_kid ? `(kid=${resp.mazbri_wire.binding_kid})` : "" }}
              </div>
            </div>
          </div>

          <div
            v-if="Object.keys(resp.mazbri_wire.binding_headers).length > 0"
            class="space-y-1.5"
          >
            <div class="font-medium text-foreground">
              请求绑定签名头 ({{ resp.mazbri_wire.invoke_url }})
            </div>
            <div class="divide-y divide-border overflow-hidden rounded-lg border border-border font-mono">
              <div
                v-for="(hv, hk) in resp.mazbri_wire.binding_headers"
                :key="hk"
                class="grid grid-cols-3 px-3.5 py-2"
              >
                <span class="font-medium text-foreground">{{ hk }}</span>
                <span class="col-span-2 break-all text-muted-foreground">{{ hv }}</span>
              </div>
            </div>
          </div>

          <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
            <div class="space-y-1.5">
              <div class="font-medium text-foreground">
                加密前内部请求载荷
              </div>
              <pre
                class="max-h-60 overflow-auto rounded-lg border border-border bg-muted/35 p-3 font-mono leading-relaxed whitespace-pre-wrap break-all text-foreground"
              >{{ JSON.stringify(resp.mazbri_wire.inner_gateway_request, null, 2) }}</pre>
            </div>
            <div class="space-y-1.5">
              <div class="font-medium text-foreground">
                实际发送信封报文
              </div>
              <pre
                class="max-h-60 overflow-auto rounded-lg border border-border bg-muted/35 p-3 font-mono leading-relaxed whitespace-pre-wrap break-all text-foreground"
              >{{ formatMaybeJson(resp.mazbri_wire.wire_request_body) }}</pre>
            </div>
          </div>

          <div
            v-if="resp.mazbri_wire.wire_response_body"
            class="grid grid-cols-1 gap-3 lg:grid-cols-2"
          >
            <div class="space-y-1.5">
              <div class="font-medium text-foreground">
                网关原始响应报文 (HTTP {{ resp.mazbri_wire.wire_response_status ?? "-" }})
              </div>
              <pre
                class="max-h-60 overflow-auto rounded-lg border border-border bg-muted/35 p-3 font-mono leading-relaxed whitespace-pre-wrap break-all text-foreground"
              >{{ formatMaybeJson(resp.mazbri_wire.wire_response_body) }}</pre>
            </div>
            <div
              v-if="
                resp.mazbri_wire.decoded_envelope !== undefined &&
                  resp.mazbri_wire.decoded_envelope !== null
              "
              class="space-y-1.5"
            >
              <div class="font-medium text-foreground">
                客户端解封响应载荷
              </div>
              <pre
                class="max-h-60 overflow-auto rounded-lg border border-border bg-muted/35 p-3 font-mono leading-relaxed whitespace-pre-wrap break-all text-foreground"
              >{{ JSON.stringify(resp.mazbri_wire.decoded_envelope, null, 2) }}</pre>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { AlertTriangle, Copy, Download, Loader2, Send, X } from "lucide-vue-next";
import {
  formatBytes,
  formatLatencyUs,
  methodColorClass,
  statusCategoryClass,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import { copyText } from "../../utils/clipboard";
import {
  activeTab,
  responsePaneOpen,
  type WorkspaceTab,
} from "../../stores/workspace";
import CodeMirrorEditor from "../common/CodeMirrorEditor.vue";
import UiBadge from "../ui/UiBadge.vue";
import UiButton from "../ui/UiButton.vue";

const resp = computed(() => activeTab.value.response);

const responseTabs = computed<
  {
    id: WorkspaceTab["responseSubTab"];
    label: string;
    count: number;
    dot: boolean;
    errorDot: boolean;
  }[]
>(() => {
  const r = resp.value;
  if (!r) return [];
  const items: {
    id: WorkspaceTab["responseSubTab"];
    label: string;
    count: number;
    dot: boolean;
    errorDot: boolean;
  }[] = [
    { id: "json", label: "JSON 视图", count: 0, dot: false, errorDot: false },
    { id: "raw", label: "原始文本", count: 0, dot: false, errorDot: false },
    {
      id: "headers",
      label: "响应头",
      count: r.headers.length,
      dot: false,
      errorDot: false,
    },
    {
      id: "assertions",
      label: "断言结果",
      count: r.assertions.length,
      dot: !r.passed_assertions,
      errorDot: !r.passed_assertions,
    },
    { id: "preview", label: "实际发出请求", count: 0, dot: false, errorDot: false },
  ];
  if (r.mazbri_wire) {
    items.push({
      id: "mazbri_wire",
      label: "Mazbri 信封与签名追踪",
      count: 0,
      dot: true,
      errorDot: false,
    });
  }
  return items;
});

const prettyBodyText = computed(() => {
  if (!resp.value) return "";
  if (resp.value.body_json !== undefined && resp.value.body_json !== null) {
    return JSON.stringify(resp.value.body_json, null, 2);
  }
  return resp.value.body_text;
});

function formatMaybeJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

async function copyResponse(): Promise<void> {
  if (!resp.value) return;
  const ok = await copyText(prettyBodyText.value);
  if (ok) {
    toast.success("响应内容已复制");
  } else {
    toast.error("复制失败");
  }
}

function downloadResponse(): void {
  if (!resp.value) return;
  const isJson =
    resp.value.body_json !== undefined && resp.value.body_json !== null;
  const blob = new Blob([prettyBodyText.value], {
    type: isJson ? "application/json" : "text/plain",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `response-${Date.now()}.${isJson ? "json" : "txt"}`;
  a.click();
  URL.revokeObjectURL(url);
}
</script>
