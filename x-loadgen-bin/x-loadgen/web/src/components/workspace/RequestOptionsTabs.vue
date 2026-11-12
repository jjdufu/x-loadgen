<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-y-auto bg-primary">
    <HoppSmartTabs v-model="activeTab.requestSubTab" render-inactive-tabs>
      <!-- Query Parameters -->
      <HoppSmartTab
        id="params"
        label="查询参数 (Params)"
        :info="activeParamsCount ? String(activeParamsCount) : null"
      >
        <KeyValueEditor
          v-model="draft.params"
          title="Query Parameters"
          key-placeholder="参数名 (如 page)"
          value-placeholder="参数值 (支持 <<var>> / {{seq}})"
          empty-text="当前请求没有附加 URL 查询参数"
        />
      </HoppSmartTab>

      <!-- Request Body -->
      <HoppSmartTab
        id="body"
        label="请求体 (Body)"
        :indicator="Boolean(draft.body.content_type)"
      >
        <div class="flex flex-col flex-1">
          <div class="flex items-center justify-between px-4 py-2 border-b border-dividerLight bg-primary">
            <div class="flex items-center space-x-2">
              <span class="text-tiny text-secondaryLight font-semibold">Content-Type</span>
              <select
                :value="draft.body.content_type ?? ''"
                class="select !py-1 !px-2.5 text-tiny font-mono w-64"
                @change="setContentType(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="ct in BODY_CONTENT_TYPES" :key="ct.value" :value="ct.value">
                  {{ ct.label }}
                </option>
              </select>
            </div>
            <span v-if="draft.protocol === 'mazbri'" class="text-tiny text-secondaryLight">
              Mazbri 模式下 JSON 请求体会封装进 <code class="font-mono text-accent">GatewayRequest.body</code>
            </span>
          </div>

          <div
            v-if="!draft.body.content_type"
            class="flex flex-col items-center justify-center py-10 text-secondaryLight text-tiny"
          >
            <p class="mb-3">此请求当前不包含请求体 (None)</p>
            <HoppButtonSecondary
              label="使用 application/json 请求体"
              outline
              @click="setContentType('application/json')"
            />
          </div>

          <KeyValueEditor
            v-else-if="isFormBody"
            v-model="draft.body.form"
            :title="draft.body.content_type"
            key-placeholder="字段名"
            value-placeholder="字段值 (支持 <<var>>)"
            empty-text="表单字段为空"
          />

          <div v-else class="p-3">
            <CodeMirrorEditor
              :model-value="draft.body.body ?? ''"
              :language="draft.body.content_type.includes('json') ? 'json' : 'text'"
              placeholder="{&quot;key&quot;: &quot;value&quot;}"
              min-height="180px"
              max-height="420px"
              @update:model-value="(v) => (draft.body.body = v)"
            />
          </div>
        </div>
      </HoppSmartTab>

      <!-- Headers -->
      <HoppSmartTab
        id="headers"
        label="请求头 (Headers)"
        :info="activeHeadersCount ? String(activeHeadersCount) : null"
      >
        <KeyValueEditor
          v-model="draft.headers"
          title="HTTP Request Headers"
          key-placeholder="Header 名称 (如 X-Trace-Id)"
          value-placeholder="Header 值 (支持 <<var>>)"
          empty-text="当前未配置自定义请求头"
        />
      </HoppSmartTab>

      <!-- Dual-Protocol Auth / Mazbri Crypto Tab -->
      <HoppSmartTab
        id="auth"
        :label="draft.protocol === 'mazbri' ? 'Mazbri 协议与加密' : '鉴权认证 (Auth)'"
        :indicator="draft.protocol === 'mazbri' || draft.auth.auth_type !== 'none'"
      >
        <!-- HTTP Auth -->
        <div v-if="draft.protocol === 'http'" class="p-4 space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-2 pb-3 border-b border-dividerLight">
            <div class="flex items-center space-x-3">
              <span class="text-tiny text-secondaryLight font-semibold">认证类型</span>
              <select v-model="draft.auth.auth_type" class="select !py-1.5 w-56">
                <option value="inherit">继承集合 / 环境默认 (Inherit)</option>
                <option value="none">无认证 (None)</option>
                <option value="bearer">Bearer Token</option>
                <option value="basic">Basic Auth</option>
                <option value="api-key">API Key</option>
              </select>
            </div>
            <HoppSmartToggle
              :on="draft.auth.auth_active"
              @change="draft.auth.auth_active = !draft.auth.auth_active"
            >
              启用当前鉴权配置
            </HoppSmartToggle>
          </div>

          <div v-if="draft.auth.auth_type === 'inherit'" class="text-tiny text-secondaryLight py-2">
            运行时将自动依次向上继承所属文件夹、根集合或当前激活环境的默认 HTTP 认证配置。
          </div>

          <div v-else-if="draft.auth.auth_type === 'bearer'" class="max-w-xl">
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              Bearer Token
            </label>
            <input
              :value="draft.auth.token ?? ''"
              type="text"
              placeholder="输入 Token 或 <<accessToken>>"
              class="input font-mono"
              @input="draft.auth.token = ($event.target as HTMLInputElement).value"
            />
          </div>

          <div v-else-if="draft.auth.auth_type === 'basic'" class="grid grid-cols-1 md:grid-cols-2 gap-3 max-w-xl">
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">用户名 (Username)</label>
              <input
                :value="draft.auth.username ?? ''"
                type="text"
                class="input font-mono"
                @input="draft.auth.username = ($event.target as HTMLInputElement).value"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">密码 (Password)</label>
              <input
                :value="draft.auth.password ?? ''"
                type="password"
                class="input font-mono"
                @input="draft.auth.password = ($event.target as HTMLInputElement).value"
              />
            </div>
          </div>

          <div v-else-if="draft.auth.auth_type === 'api-key'" class="grid grid-cols-1 md:grid-cols-3 gap-3 max-w-2xl">
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">Key 名称</label>
              <input
                :value="draft.auth.key ?? ''"
                type="text"
                placeholder="X-API-Key"
                class="input font-mono"
                @input="draft.auth.key = ($event.target as HTMLInputElement).value"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">Key 值</label>
              <input
                :value="draft.auth.value ?? ''"
                type="text"
                placeholder="<<apiKey>>"
                class="input font-mono"
                @input="draft.auth.value = ($event.target as HTMLInputElement).value"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">注入位置</label>
              <select
                :value="draft.auth.add_to ?? 'HEADERS'"
                class="select"
                @change="draft.auth.add_to = ($event.target as HTMLSelectElement).value"
              >
                <option value="HEADERS">请求头 (Headers)</option>
                <option value="QUERY_PARAMS">URL 参数 (Query)</option>
              </select>
            </div>
          </div>
        </div>

        <!-- Mazbri Protocol & Crypto Config -->
        <div v-else class="p-4 space-y-4">
          <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                Mazbri 产品标识 (product_id) *
              </label>
              <input
                v-model="draft.mazbri.product_id"
                type="text"
                placeholder="例如 prod.demo.echo (支持 <<productId>>)"
                class="input font-mono"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                网关地址覆盖 (gateway_base_url，留空则使用当前激活环境的默认网关)
              </label>
              <input
                :value="draft.mazbri.gateway_base_url ?? ''"
                type="text"
                :placeholder="activeEnvironmentDetail?.gateway_base_url || 'http://127.0.0.1:18080'"
                class="input font-mono"
                @input="draft.mazbri.gateway_base_url = ($event.target as HTMLInputElement).value"
              />
            </div>
          </div>

          <div class="p-3 rounded border border-dividerLight bg-primaryLight/30 space-y-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <span class="font-semibold text-secondaryDark">
                Mazbri 租户凭据与信封密钥来源 (Tenant Source)
              </span>
              <div class="flex items-center space-x-1 bg-primary rounded border border-divider p-0.5">
                <button
                  type="button"
                  class="px-3 py-1 rounded text-tiny font-semibold transition"
                  :class="
                    draft.mazbri.tenant_source === 'environment'
                      ? 'bg-accent text-accentContrast'
                      : 'text-secondary hover:text-secondaryDark'
                  "
                  @click="setTenantSource('environment')"
                >
                  使用环境加权租户池 (推荐)
                </button>
                <button
                  type="button"
                  class="px-3 py-1 rounded text-tiny font-semibold transition"
                  :class="
                    draft.mazbri.tenant_source === 'inline'
                      ? 'bg-accent text-accentContrast'
                      : 'text-secondary hover:text-secondaryDark'
                  "
                  @click="setTenantSource('inline')"
                >
                  请求内联独立租户凭据
                </button>
              </div>
            </div>

            <div v-if="draft.mazbri.tenant_source === 'environment'" class="space-y-3">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="text-tiny text-secondary">
                  当前激活环境：
                  <strong class="text-secondaryDark">
                    {{ activeEnvironmentDetail?.name ?? "未选择命名环境" }}
                  </strong>
                  （共 {{ activeEnvironmentDetail?.tenants.length ?? 0 }} 个租户）
                </div>
                <HoppButtonSecondary
                  label="管理环境租户密钥池"
                  outline
                  @click="openEnvironmentEditor(activeEnvironmentId)"
                />
              </div>

              <div v-if="activeEnvironmentDetail?.tenants.length" class="max-w-md">
                <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                  指定固定租户（可选，留空则按权重自动分配）
                </label>
                <select
                  :value="draft.mazbri.tenant_name ?? ''"
                  class="select font-mono"
                  @change="draft.mazbri.tenant_name = ($event.target as HTMLSelectElement).value || null"
                >
                  <option value="">按权重轮询池中全部租户</option>
                  <option
                    v-for="t in activeEnvironmentDetail.tenants"
                    :key="t.id || t.name"
                    :value="t.name"
                  >
                    {{ t.name }} (mode={{ t.encryption_mode }}, weight={{ t.weight }})
                  </option>
                </select>
              </div>
            </div>

            <div v-else>
              <TenantCryptoForm
                :model-value="inlineTenantModel"
                @update:model-value="(val) => (draft.mazbri.inline_tenant = val)"
              />
            </div>
          </div>
        </div>
      </HoppSmartTab>

      <!-- Assertions -->
      <HoppSmartTab
        id="assertions"
        label="响应断言 (Tests)"
        :info="draft.assertions.checks.length ? String(draft.assertions.checks.length) : null"
      >
        <div class="p-4 space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-2 pb-3 border-b border-dividerLight">
            <div class="flex items-center space-x-3">
              <span class="text-tiny text-secondaryLight font-semibold">传输层期望结果</span>
              <select v-model="draft.assertions.expect_transport" class="select !py-1.5 w-64">
                <option value="ok">期望成功响应 (ok / HTTP 2xx)</option>
                <option value="reject">期望网关拦截拒绝 (reject / 负向安全测试)</option>
              </select>
            </div>
            <HoppButtonSecondary :icon="Plus" label="添加 JSON 字段断言" outline @click="addCheck" />
          </div>

          <p class="text-tiny text-secondaryLight">
            支持点分路径与数组下标（如 <code class="font-mono text-accent">user.id</code> 或
            <code class="font-mono text-accent">items[0].status</code>）。Mazbri 请求自动对解封后的业务
            <code class="font-mono">data</code> 载荷执行校验。
          </p>

          <div v-if="draft.assertions.checks.length === 0" class="py-6 text-center text-secondaryLight text-tiny">
            未添加业务字段断言（仅校验传输层状态）
          </div>

          <div v-else class="divide-y divide-dividerLight border border-dividerLight rounded">
            <div
              v-for="(chk, idx) in draft.assertions.checks"
              :key="idx"
              class="flex items-center divide-x divide-dividerLight"
            >
              <div class="flex-1">
                <input
                  v-model="chk.path"
                  type="text"
                  placeholder="JSON 字段路径，如 code 或 result.ok"
                  class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark"
                />
              </div>
              <div class="w-40">
                <select
                  :value="chk.eq === undefined ? 'exists' : 'eq'"
                  class="w-full px-2 py-1.5 bg-transparent text-tiny cursor-pointer text-secondaryDark"
                  @change="toggleCheckOp(idx, ($event.target as HTMLSelectElement).value)"
                >
                  <option value="exists" class="bg-primary">存在且非 null</option>
                  <option value="eq" class="bg-primary">等于 JSON 值 (==)</option>
                </select>
              </div>
              <div class="flex-1">
                <input
                  v-if="chk.eq !== undefined"
                  :value="formatEqInput(chk.eq)"
                  type="text"
                  placeholder="例如 &quot;ok&quot;、200、true"
                  class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark"
                  @change="updateCheckEq(idx, ($event.target as HTMLInputElement).value)"
                />
                <span v-else class="block px-3 py-1.5 text-tiny text-secondaryLight italic">
                  只要路径存在且不为 null 即通过
                </span>
              </div>
              <div class="w-9 flex justify-center">
                <HoppButtonSecondary
                  :icon="Trash"
                  color="red"
                  @click="draft.assertions.checks.splice(idx, 1)"
                />
              </div>
            </div>
          </div>
        </div>
      </HoppSmartTab>

      <!-- Request-level TLS Override -->
      <HoppSmartTab
        id="tls"
        label="TLS / 超时"
        :indicator="draft.tls.insecure_skip_verify || Boolean(draft.tls.ca_cert || draft.tls.client_cert)"
      >
        <div class="p-4 space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-4">
            <HoppSmartToggle
              :on="draft.tls.insecure_skip_verify"
              @change="draft.tls.insecure_skip_verify = !draft.tls.insecure_skip_verify"
            >
              跳过服务端 TLS 证书校验 (覆盖环境默认设置)
            </HoppSmartToggle>
            <div class="flex items-center space-x-2">
              <span class="text-tiny text-secondaryLight">单请求独立超时 (秒)</span>
              <input
                :value="draft.timeout_secs ?? ''"
                type="number"
                min="1"
                placeholder="默认跟随压测配置"
                class="input font-mono w-40 !py-1"
                @input="
                  draft.timeout_secs = ($event.target as HTMLInputElement).value
                    ? Math.max(1, Number(($event.target as HTMLInputElement).value))
                    : null
                "
              />
            </div>
          </div>

          <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">自定义 CA PEM / 路径</label>
              <textarea
                :value="draft.tls.ca_cert ?? ''"
                rows="3"
                class="textarea font-mono text-tiny"
                placeholder="留空则继承环境 TLS"
                @input="draft.tls.ca_cert = ($event.target as HTMLTextAreaElement).value"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">mTLS 客户端证书</label>
              <textarea
                :value="draft.tls.client_cert ?? ''"
                rows="3"
                class="textarea font-mono text-tiny"
                @input="draft.tls.client_cert = ($event.target as HTMLTextAreaElement).value"
              />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">mTLS 客户端私钥</label>
              <textarea
                :value="draft.tls.client_key ?? ''"
                rows="3"
                class="textarea font-mono text-tiny"
                @input="draft.tls.client_key = ($event.target as HTMLTextAreaElement).value"
              />
            </div>
          </div>
        </div>
      </HoppSmartTab>
    </HoppSmartTabs>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  HoppButtonSecondary,
  HoppSmartTab,
  HoppSmartTabs,
  HoppSmartToggle,
} from "@hoppscotch/ui";
import { Plus, Trash } from "lucide-vue-next";
import type { TenantSpec } from "../../api/types";
import { BODY_CONTENT_TYPES, createDefaultTenant } from "../../helpers/format";
import {
  activeEnvironmentDetail,
  activeEnvironmentId,
  activeTab,
  openEnvironmentEditor,
} from "../../stores/workspace";
import CodeMirrorEditor from "../common/CodeMirrorEditor.vue";
import KeyValueEditor from "../common/KeyValueEditor.vue";
import TenantCryptoForm from "../common/TenantCryptoForm.vue";

const draft = computed(() => activeTab.value.draft);

const activeParamsCount = computed(
  () => draft.value.params.filter((p) => p.active && p.key.trim()).length,
);
const activeHeadersCount = computed(
  () => draft.value.headers.filter((h) => h.active && h.key.trim()).length,
);

const isFormBody = computed(
  () =>
    draft.value.body.content_type === "application/x-www-form-urlencoded" ||
    draft.value.body.content_type === "multipart/form-data",
);

const inlineTenantModel = computed<TenantSpec>(() => {
  if (!draft.value.mazbri.inline_tenant) {
    return createDefaultTenant("inline-tenant");
  }
  return draft.value.mazbri.inline_tenant;
});

function setContentType(ct: string): void {
  draft.value.body.content_type = ct || null;
  if (ct === "application/json" && !draft.value.body.body) {
    draft.value.body.body = "{\n  \n}";
  }
}

function setTenantSource(source: "environment" | "inline"): void {
  draft.value.mazbri.tenant_source = source;
  if (source === "inline" && !draft.value.mazbri.inline_tenant) {
    draft.value.mazbri.inline_tenant = createDefaultTenant("inline-tenant");
  }
}

function addCheck(): void {
  draft.value.assertions.checks.push({ path: "" });
}

function toggleCheckOp(idx: number, op: string): void {
  const row = draft.value.assertions.checks[idx];
  if (!row) return;
  if (op === "exists") {
    delete row.eq;
  } else {
    row.eq = "ok";
  }
}

function formatEqInput(val: unknown): string {
  if (typeof val === "string") return JSON.stringify(val);
  return JSON.stringify(val);
}

function updateCheckEq(idx: number, raw: string): void {
  const row = draft.value.assertions.checks[idx];
  if (!row) return;
  const trimmed = raw.trim();
  try {
    row.eq = JSON.parse(trimmed);
  } catch {
    row.eq = trimmed;
  }
}
</script>
