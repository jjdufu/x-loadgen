<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-hidden bg-primary">
    <!-- Loading State -->
    <div
      v-if="activeTab.sending"
      class="flex flex-col items-center justify-center flex-1 p-8 space-y-3"
    >
      <HoppSmartSpinner />
      <span class="text-tiny text-secondaryLight">正在发送请求并解析响应...</span>
    </div>

    <!-- Empty Initial State (Hoppscotch Shortcut Placeholder) -->
    <div
      v-else-if="!resp"
      class="flex flex-col items-center justify-center flex-1 p-8 text-secondaryLight select-none space-y-4"
    >
      <Send class="w-10 h-10 opacity-30 text-accent" />
      <div class="text-center space-y-1">
        <p class="font-semibold text-secondaryDark">输入地址并点击「发送 (Send)」调试单次请求，或点击「压测」直接启动高并发压测</p>
        <p class="text-tiny">
          发送单次调试 <span class="shortcut-key">Ctrl Enter</span> · 保存请求
          <span class="shortcut-key">Ctrl S</span>
        </p>
      </div>
    </div>

    <!-- Active Probe Response -->
    <template v-else>
      <!-- Hoppscotch ResponseMeta Sticky Bar -->
      <div
        class="flex flex-wrap items-center justify-between gap-2 px-4 py-2 border-b border-dividerLight bg-primary text-tiny"
      >
        <div class="flex flex-wrap items-center gap-4 font-semibold">
          <div class="flex items-center space-x-1.5">
            <span class="text-secondaryLight">状态:</span>
            <span :class="statusCategoryClass(resp.http_status, resp.transport_ok)">
              {{
                resp.http_status
                  ? `${resp.http_status} ${resp.status_text ?? ""}`
                  : resp.error_class || "ERR"
              }}
            </span>
          </div>

          <div class="flex items-center space-x-1.5">
            <span class="text-secondaryLight">耗时:</span>
            <span class="font-mono" :class="statusCategoryClass(resp.http_status, resp.transport_ok)">
              {{ formatLatencyUs(resp.latency_us) }}
            </span>
          </div>

          <div class="flex items-center space-x-1.5">
            <span class="text-secondaryLight">大小:</span>
            <span class="font-mono" :class="statusCategoryClass(resp.http_status, resp.transport_ok)">
              {{ formatBytes(resp.response_bytes) }}
            </span>
          </div>

          <span
            class="px-2 py-0.5 rounded-full font-mono text-[11px] border"
            :class="
              resp.passed_assertions
                ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-400'
                : 'border-rose-500/40 bg-rose-500/10 text-rose-400'
            "
          >
            {{ resp.passed_assertions ? "✓ 断言通过" : "✕ 断言未通过" }}
          </span>
        </div>

        <div class="flex items-center space-x-1">
          <HoppButtonSecondary
            v-tippy="'复制响应正文'"
            :icon="Copy"
            @click="copyResponse"
          />
          <HoppButtonSecondary
            v-tippy="'下载响应内容'"
            :icon="Download"
            @click="downloadResponse"
          />
        </div>
      </div>

      <!-- Unresolved <<var>> Warning Banner -->
      <div
        v-if="resp.request_preview.unresolved_variables.length > 0"
        class="px-4 py-1.5 bg-amber-500/15 border-b border-amber-500/30 text-amber-400 text-tiny flex items-center space-x-2"
      >
        <AlertTriangle class="w-4 h-4 flex-shrink-0" />
        <span>
          检测到未解析的模板变量：
          <code class="font-mono font-bold">
            {{ resp.request_preview.unresolved_variables.join(", ") }}
          </code>
          （请检查右上角是否已选择对应环境）
        </span>
      </div>

      <!-- Error Banner -->
      <div
        v-if="resp.error"
        class="px-4 py-2 bg-rose-500/15 border-b border-rose-500/30 text-rose-400 text-tiny font-mono"
      >
        [{{ resp.error_class || "transport_error" }}] {{ resp.error }}
      </div>

      <!-- Response Tabs -->
      <div class="flex flex-col flex-1 min-h-0 overflow-y-auto">
        <HoppSmartTabs v-model="activeTab.responseSubTab" render-inactive-tabs>
          <HoppSmartTab id="json" label="JSON 视图">
            <div class="p-3">
              <CodeMirrorEditor
                :model-value="prettyBodyText"
                :language="resp.body_json !== undefined && resp.body_json !== null ? 'json' : 'text'"
                readonly
                min-height="200px"
                max-height="520px"
              />
            </div>
          </HoppSmartTab>

          <HoppSmartTab id="raw" label="原始文本 (Raw)">
            <div class="p-3">
              <pre
                class="p-3 rounded border border-dividerLight bg-primaryLight font-mono text-tiny text-secondaryDark whitespace-pre-wrap break-all select-text max-h-[520px] overflow-auto"
              >{{ resp.body_text || "(空响应体)" }}</pre>
            </div>
          </HoppSmartTab>

          <HoppSmartTab
            id="headers"
            label="响应头 (Headers)"
            :info="resp.headers.length ? String(resp.headers.length) : null"
          >
            <div class="divide-y divide-dividerLight border-b border-dividerLight font-mono text-tiny select-text">
              <div
                v-for="([k, v], idx) in resp.headers"
                :key="idx"
                class="grid grid-cols-3 px-4 py-2 hover:bg-primaryLight/50"
              >
                <span class="text-secondaryLight truncate pr-3">{{ k }}</span>
                <span class="col-span-2 text-secondaryDark break-all">{{ v }}</span>
              </div>
            </div>
          </HoppSmartTab>

          <HoppSmartTab
            id="assertions"
            label="断言明细 (Results)"
            :info="resp.assertions.length ? String(resp.assertions.length) : null"
            :indicator="!resp.passed_assertions"
            indicator-variant="error"
          >
            <div class="p-4 space-y-2">
              <div
                class="px-3 py-2 rounded border text-tiny font-semibold"
                :class="
                  resp.passed_assertions
                    ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-400'
                    : 'border-rose-500/40 bg-rose-500/10 text-rose-400'
                "
              >
                传输与业务综合判定：{{ resp.passed_assertions ? "全部通过 (PASS)" : "未通过 (FAIL)" }}
              </div>

              <div
                v-for="(item, idx) in resp.assertions"
                :key="idx"
                class="p-3 rounded border border-dividerLight bg-primaryLight/40 flex items-start justify-between gap-3 text-tiny font-mono"
              >
                <div class="space-y-1">
                  <div class="flex items-center space-x-2">
                    <span :class="item.passed ? 'text-emerald-400' : 'text-rose-400'">
                      {{ item.passed ? "✓ PASS" : "✕ FAIL" }}
                    </span>
                    <span class="text-secondaryDark font-bold">{{ item.path }}</span>
                  </div>
                  <div v-if="item.expected !== undefined" class="text-secondaryLight">
                    期望值: <span class="text-secondaryDark">{{ JSON.stringify(item.expected) }}</span>
                  </div>
                  <div class="text-secondaryLight">
                    实际值: <span class="text-secondaryDark">{{ JSON.stringify(item.actual) }}</span>
                  </div>
                  <div v-if="item.message" class="text-rose-400">{{ item.message }}</div>
                </div>
              </div>
            </div>
          </HoppSmartTab>

          <HoppSmartTab id="preview" label="实际发出请求 (Sent)">
            <div class="p-4 space-y-3 text-tiny font-mono select-text">
              <div class="p-3 rounded border border-dividerLight bg-primaryLight">
                <span class="font-bold text-accent mr-2">{{ resp.request_preview.method }}</span>
                <span class="text-secondaryDark break-all">{{ resp.request_preview.url }}</span>
              </div>

              <div v-if="resp.request_preview.headers.length" class="rounded border border-dividerLight divide-y divide-dividerLight">
                <div
                  v-for="([hk, hv], idx) in resp.request_preview.headers"
                  :key="idx"
                  class="grid grid-cols-3 px-3 py-1.5"
                >
                  <span class="text-secondaryLight">{{ hk }}</span>
                  <span class="col-span-2 text-secondaryDark break-all">{{ hv }}</span>
                </div>
              </div>

              <div v-if="resp.request_preview.body_preview">
                <div class="text-secondaryLight mb-1 font-sans font-semibold">发出载荷预览</div>
                <pre class="p-3 rounded border border-dividerLight bg-primaryLight whitespace-pre-wrap break-all text-secondaryDark">{{ resp.request_preview.body_preview }}</pre>
              </div>
            </div>
          </HoppSmartTab>

          <!-- Mazbri Cryptographic Wire Inspector -->
          <HoppSmartTab
            v-if="resp.mazbri_wire"
            id="mazbri_wire"
            label="Mazbri 信封与签名追踪"
            indicator
          >
            <div class="p-4 space-y-4 text-tiny select-text">
              <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
                <div class="p-2.5 rounded border border-dividerLight bg-primaryLight">
                  <div class="text-secondaryLight">命中租户</div>
                  <div class="font-mono font-bold text-secondaryDark mt-0.5">
                    {{ resp.mazbri_wire.tenant_name }}
                  </div>
                </div>
                <div class="p-2.5 rounded border border-dividerLight bg-primaryLight">
                  <div class="text-secondaryLight">加密模式 / 套件</div>
                  <div class="font-mono font-bold text-accent mt-0.5">
                    {{ resp.mazbri_wire.encryption_mode }}
                    {{ resp.mazbri_wire.suite ? `· ${resp.mazbri_wire.suite}` : "" }}
                  </div>
                </div>
                <div class="p-2.5 rounded border border-dividerLight bg-primaryLight">
                  <div class="text-secondaryLight">/auth/token 换票耗时</div>
                  <div class="font-mono font-bold text-secondaryDark mt-0.5">
                    {{ formatLatencyUs(resp.mazbri_wire.token_exchange_us) }}
                  </div>
                </div>
                <div class="p-2.5 rounded border border-dividerLight bg-primaryLight">
                  <div class="text-secondaryLight">请求绑定签名</div>
                  <div class="font-mono font-bold text-secondaryDark mt-0.5">
                    {{ resp.mazbri_wire.binding_alg || "未启用" }}
                    {{ resp.mazbri_wire.binding_kid ? `(kid=${resp.mazbri_wire.binding_kid})` : "" }}
                  </div>
                </div>
              </div>

              <div
                v-if="Object.keys(resp.mazbri_wire.binding_headers).length > 0"
                class="space-y-1.5"
              >
                <div class="font-semibold text-secondaryDark">
                  x-mazbri-* 请求绑定签名头 ({{ resp.mazbri_wire.invoke_url }})
                </div>
                <div class="rounded border border-dividerLight divide-y divide-dividerLight font-mono">
                  <div
                    v-for="(hv, hk) in resp.mazbri_wire.binding_headers"
                    :key="hk"
                    class="grid grid-cols-3 px-3 py-1.5"
                  >
                    <span class="text-accent">{{ hk }}</span>
                    <span class="col-span-2 text-secondaryDark break-all">{{ hv }}</span>
                  </div>
                </div>
              </div>

              <div class="grid grid-cols-1 lg:grid-cols-2 gap-3">
                <div>
                  <div class="font-semibold text-secondaryDark mb-1">
                    内部明文 GatewayRequest JSON (加密前)
                  </div>
                  <pre class="p-3 rounded border border-dividerLight bg-primaryLight font-mono whitespace-pre-wrap break-all max-h-60 overflow-auto text-secondaryDark">{{ JSON.stringify(resp.mazbri_wire.inner_gateway_request, null, 2) }}</pre>
                </div>
                <div>
                  <div class="font-semibold text-secondaryDark mb-1">
                    线上实际发送信封报文 (Wire Request Envelope)
                  </div>
                  <pre class="p-3 rounded border border-dividerLight bg-primaryLight font-mono whitespace-pre-wrap break-all max-h-60 overflow-auto text-secondaryDark">{{ formatMaybeJson(resp.mazbri_wire.wire_request_body) }}</pre>
                </div>
              </div>

              <div v-if="resp.mazbri_wire.wire_response_body" class="grid grid-cols-1 lg:grid-cols-2 gap-3">
                <div>
                  <div class="font-semibold text-secondaryDark mb-1">
                    网关原始响应报文 (Wire Response · HTTP {{ resp.mazbri_wire.wire_response_status ?? "-" }})
                  </div>
                  <pre class="p-3 rounded border border-dividerLight bg-primaryLight font-mono whitespace-pre-wrap break-all max-h-60 overflow-auto text-secondaryDark">{{ formatMaybeJson(resp.mazbri_wire.wire_response_body) }}</pre>
                </div>
                <div v-if="resp.mazbri_wire.decoded_envelope !== undefined && resp.mazbri_wire.decoded_envelope !== null">
                  <div class="font-semibold text-secondaryDark mb-1">
                    客户端解封后 GatewayResponseEnvelope
                  </div>
                  <pre class="p-3 rounded border border-dividerLight bg-primaryLight font-mono whitespace-pre-wrap break-all max-h-60 overflow-auto text-secondaryDark">{{ JSON.stringify(resp.mazbri_wire.decoded_envelope, null, 2) }}</pre>
                </div>
              </div>
            </div>
          </HoppSmartTab>
        </HoppSmartTabs>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  HoppButtonSecondary,
  HoppSmartSpinner,
  HoppSmartTab,
  HoppSmartTabs,
} from "@hoppscotch/ui";
import { AlertTriangle, Copy, Download, Send } from "lucide-vue-next";
import {
  formatBytes,
  formatLatencyUs,
  statusCategoryClass,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import { activeTab } from "../../stores/workspace";
import CodeMirrorEditor from "../common/CodeMirrorEditor.vue";

const resp = computed(() => activeTab.value.response);

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
  try {
    await navigator.clipboard.writeText(prettyBodyText.value);
    toast.success("响应内容已复制");
  } catch {
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
