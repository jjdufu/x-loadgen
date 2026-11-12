<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-background">
    <!-- Crisp Shadcn / Geist Underline Tab Strip -->
    <div
      class="flex h-10 shrink-0 items-center gap-1 overflow-x-auto border-b border-border bg-background px-4 select-none"
      role="tablist"
    >
      <button
        v-for="tab in subTabs"
        :key="tab.id"
        type="button"
        role="tab"
        :aria-selected="activeTab.requestSubTab === tab.id"
        class="relative inline-flex h-full items-center gap-1.5 px-3 text-xs font-medium whitespace-nowrap transition-colors"
        :class="
          activeTab.requestSubTab === tab.id
            ? 'text-foreground'
            : 'text-muted-foreground hover:text-foreground'
        "
        @click="activeTab.requestSubTab = tab.id"
      >
        <span>{{ tab.label }}</span>
        <span
          v-if="tab.count"
          class="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-foreground tabular-nums"
        >
          {{ tab.count }}
        </span>
        <span
          v-else-if="tab.dot"
          class="h-1.5 w-1.5 rounded-full bg-foreground/75"
        />
        <span
          v-if="activeTab.requestSubTab === tab.id"
          class="absolute inset-x-2 bottom-0 h-[2px] rounded-full bg-foreground"
        />
      </button>
    </div>

    <!-- Active Option Tab Body -->
    <div class="min-h-0 flex-1 overflow-y-auto">
      <!-- 1. Query Parameters -->
      <KeyValueEditor
        v-if="activeTab.requestSubTab === 'params'"
        v-model="draft.params"
        title="Query Parameters"
        key-placeholder="参数名 (如 page)"
        value-placeholder="参数值 (支持 <<var>> / {{seq}})"
        empty-text="当前请求没有附加 URL 查询参数"
      />

      <!-- 2. Request Body -->
      <div v-else-if="activeTab.requestSubTab === 'body'" class="flex flex-1 flex-col">
        <div
          class="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-surface/50 px-4 py-2"
        >
          <div class="flex items-center gap-2.5">
            <span class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              Content-Type
            </span>
            <UiSelect
              :model-value="draft.body.content_type ?? ''"
              size="sm"
              mono
              wrapper-class="w-64"
              @update:model-value="setContentType"
            >
              <option v-for="ct in BODY_CONTENT_TYPES" :key="ct.value" :value="ct.value">
                {{ ct.label }}
              </option>
            </UiSelect>
          </div>

          <span v-if="draft.protocol === 'mazbri'" class="text-xs text-muted-foreground">
            Mazbri 模式下载荷将密封进
            <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-foreground">
              GatewayRequest.body
            </code>
          </span>
        </div>

        <div
          v-if="!draft.body.content_type"
          class="flex flex-col items-center justify-center py-12 text-muted-foreground"
        >
          <p class="mb-3 text-sm">此请求当前不包含请求体 (None)</p>
          <UiButton
            variant="outline"
            size="sm"
            @click="setContentType('application/json')"
          >
            使用 application/json 请求体
          </UiButton>
        </div>

        <KeyValueEditor
          v-else-if="isFormBody"
          v-model="draft.body.form"
          :title="draft.body.content_type"
          key-placeholder="字段名"
          value-placeholder="字段值 (支持 <<var>>)"
          empty-text="表单字段为空"
        />

        <div v-else class="p-4">
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

      <!-- 3. Headers -->
      <KeyValueEditor
        v-else-if="activeTab.requestSubTab === 'headers'"
        v-model="draft.headers"
        title="HTTP Request Headers"
        key-placeholder="Header 名称 (如 X-Trace-Id)"
        value-placeholder="Header 值 (支持 <<var>>)"
        empty-text="当前未配置自定义请求头"
      />

      <!-- 4. Auth / Mazbri Crypto -->
      <div v-else-if="activeTab.requestSubTab === 'auth'" class="p-5">
        <!-- HTTP Auth -->
        <div v-if="draft.protocol === 'http'" class="space-y-5">
          <div
            class="flex flex-wrap items-center justify-between gap-4 border-b border-border pb-4"
          >
            <div class="flex items-center gap-3">
              <label class="text-sm font-medium text-foreground whitespace-nowrap">
                认证类型
              </label>
              <UiSelect v-model="draft.auth.auth_type" wrapper-class="w-64">
                <option value="inherit">继承集合 / 环境默认 (Inherit)</option>
                <option value="none">无认证 (None)</option>
                <option value="bearer">Bearer Token</option>
                <option value="basic">Basic Auth</option>
                <option value="api-key">API Key</option>
              </UiSelect>
            </div>

            <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground whitespace-nowrap">
              <UiSwitch v-model="draft.auth.auth_active" />
              <span>启用当前鉴权配置</span>
            </label>
          </div>

          <div
            v-if="draft.auth.auth_type === 'inherit'"
            class="rounded-lg border border-border bg-muted/35 p-4 text-sm text-muted-foreground"
          >
            运行时将自动依次向上继承所属文件夹、根集合或当前激活环境的默认 HTTP 认证配置。
          </div>

          <div
            v-else-if="draft.auth.auth_type === 'none'"
            class="py-4 text-sm text-muted-foreground"
          >
            当前请求不携带额外鉴权凭据。
          </div>

          <div v-else-if="draft.auth.auth_type === 'bearer'" class="max-w-xl space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              Bearer Token
            </label>
            <UiInput
              :model-value="draft.auth.token ?? ''"
              mono
              placeholder="输入 Token 或 <<accessToken>>"
              @update:model-value="(v) => (draft.auth.token = v)"
            />
          </div>

          <div
            v-else-if="draft.auth.auth_type === 'basic'"
            class="grid max-w-xl grid-cols-1 gap-4 md:grid-cols-2"
          >
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                用户名 (Username)
              </label>
              <UiInput
                :model-value="draft.auth.username ?? ''"
                mono
                @update:model-value="(v) => (draft.auth.username = v)"
              />
            </div>
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                密码 (Password)
              </label>
              <UiInput
                :model-value="draft.auth.password ?? ''"
                type="password"
                mono
                @update:model-value="(v) => (draft.auth.password = v)"
              />
            </div>
          </div>

          <div
            v-else-if="draft.auth.auth_type === 'api-key'"
            class="grid max-w-2xl grid-cols-1 gap-4 md:grid-cols-3"
          >
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                Key 名称
              </label>
              <UiInput
                :model-value="draft.auth.key ?? ''"
                mono
                placeholder="X-API-Key"
                @update:model-value="(v) => (draft.auth.key = v)"
              />
            </div>
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                Key 值
              </label>
              <UiInput
                :model-value="draft.auth.value ?? ''"
                mono
                placeholder="<<apiKey>>"
                @update:model-value="(v) => (draft.auth.value = v)"
              />
            </div>
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                注入位置
              </label>
              <UiSelect
                :model-value="draft.auth.add_to ?? 'HEADERS'"
                @update:model-value="(v) => (draft.auth.add_to = v)"
              >
                <option value="HEADERS">请求头 (Headers)</option>
                <option value="QUERY_PARAMS">URL 参数 (Query)</option>
              </UiSelect>
            </div>
          </div>
        </div>

        <!-- Mazbri Protocol & Crypto Config -->
        <div v-else class="space-y-5">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                Mazbri 产品标识 (product_id) *
              </label>
              <UiInput
                v-model="draft.mazbri.product_id"
                mono
                placeholder="例如 prod.demo.echo (支持 <<productId>>)"
              />
            </div>
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                网关地址覆盖 (gateway_base_url，留空则继承激活环境)
              </label>
              <UiInput
                :model-value="draft.mazbri.gateway_base_url ?? ''"
                mono
                :placeholder="
                  activeEnvironmentDetail?.gateway_base_url || 'http://127.0.0.1:18080'
                "
                @update:model-value="(v) => (draft.mazbri.gateway_base_url = v)"
              />
            </div>
          </div>

          <div class="rounded-lg border border-border bg-card p-4 space-y-4">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <span class="text-sm font-semibold text-foreground">
                Mazbri 租户凭据与密钥池来源
              </span>
              <div class="inline-flex h-8 items-center rounded-md border border-border bg-muted p-0.5">
                <button
                  type="button"
                  class="h-full rounded px-3 text-xs font-medium whitespace-nowrap transition-all"
                  :class="
                    draft.mazbri.tenant_source === 'environment'
                      ? 'bg-background text-foreground shadow-sm'
                      : 'text-muted-foreground hover:text-foreground'
                  "
                  @click="setTenantSource('environment')"
                >
                  使用环境加权租户池 (推荐)
                </button>
                <button
                  type="button"
                  class="h-full rounded px-3 text-xs font-medium whitespace-nowrap transition-all"
                  :class="
                    draft.mazbri.tenant_source === 'inline'
                      ? 'bg-background text-foreground shadow-sm'
                      : 'text-muted-foreground hover:text-foreground'
                  "
                  @click="setTenantSource('inline')"
                >
                  请求内联独立租户凭据
                </button>
              </div>
            </div>

            <div v-if="draft.mazbri.tenant_source === 'environment'" class="space-y-3">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="text-sm text-muted-foreground">
                  当前激活环境：
                  <strong class="font-medium text-foreground">
                    {{ activeEnvironmentDetail?.name ?? "未选择命名环境" }}
                  </strong>
                  <span class="ml-1 text-xs">
                    （共 {{ activeEnvironmentDetail?.tenants.length ?? 0 }} 个租户）
                  </span>
                </div>
                <UiButton
                  variant="outline"
                  size="sm"
                  @click="openEnvironmentEditor(activeEnvironmentId)"
                >
                  管理环境租户密钥池
                </UiButton>
              </div>

              <div v-if="activeEnvironmentDetail?.tenants.length" class="max-w-md space-y-1.5">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  指定固定租户（可选，留空则按权重自动轮询）
                </label>
                <UiSelect
                  :model-value="draft.mazbri.tenant_name ?? ''"
                  mono
                  @update:model-value="(v) => (draft.mazbri.tenant_name = v || null)"
                >
                  <option value="">按权重轮询池中全部租户</option>
                  <option
                    v-for="t in activeEnvironmentDetail.tenants"
                    :key="t.id || t.name"
                    :value="t.name"
                  >
                    {{ t.name }} (mode={{ t.encryption_mode }}, weight={{ t.weight }})
                  </option>
                </UiSelect>
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
      </div>

      <!-- 5. Assertions -->
      <div v-else-if="activeTab.requestSubTab === 'assertions'" class="p-5 space-y-4">
        <div
          class="flex flex-wrap items-center justify-between gap-3 border-b border-border pb-4"
        >
          <div class="flex items-center gap-3">
            <label class="text-sm font-medium text-foreground whitespace-nowrap">
              传输层期望状态
            </label>
            <UiSelect v-model="draft.assertions.expect_transport" wrapper-class="w-72">
              <option value="ok">期望成功响应 (ok / HTTP 2xx)</option>
              <option value="reject">期望网关拦截拒绝 (reject / 负向安全测试)</option>
            </UiSelect>
          </div>
          <UiButton variant="outline" size="sm" @click="addCheck">
            <Plus class="h-3.5 w-3.5" />
            <span>添加 JSON 字段断言</span>
          </UiButton>
        </div>

        <p class="text-xs text-muted-foreground leading-relaxed">
          支持点分路径与数组下标（如
          <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">user.id</code> 或
          <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">items[0].status</code>）。Mazbri
          请求自动对客户端解封后的业务 <code class="font-mono text-foreground">data</code> 载荷执行校验。
        </p>

        <div
          v-if="draft.assertions.checks.length === 0"
          class="rounded-lg border border-dashed border-border py-8 text-center text-sm text-muted-foreground"
        >
          未添加业务字段断言（仅校验传输层状态）
        </div>

        <div v-else class="divide-y divide-border overflow-hidden rounded-lg border border-border">
          <div
            v-for="(chk, idx) in draft.assertions.checks"
            :key="idx"
            class="flex items-center divide-x divide-border bg-background"
          >
            <div class="flex-1 min-w-0">
              <input
                v-model="chk.path"
                type="text"
                placeholder="JSON 字段路径，如 status 或 result.ok"
                class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
            </div>
            <div class="w-44 shrink-0">
              <select
                :value="chk.eq === undefined ? 'exists' : 'eq'"
                class="h-9 w-full cursor-pointer bg-transparent px-2.5 text-xs text-foreground focus:outline-none"
                @change="toggleCheckOp(idx, ($event.target as HTMLSelectElement).value)"
              >
                <option value="exists" class="bg-background">存在且非 null</option>
                <option value="eq" class="bg-background">等于 JSON 值 (==)</option>
              </select>
            </div>
            <div class="flex-1 min-w-0">
              <input
                v-if="chk.eq !== undefined"
                :value="formatEqInput(chk.eq)"
                type="text"
                placeholder="例如 &quot;ok&quot;、200、true"
                class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
                @change="updateCheckEq(idx, ($event.target as HTMLInputElement).value)"
              />
              <span v-else class="block px-3 text-xs italic text-muted-foreground">
                只要路径存在且不为 null 即通过
              </span>
            </div>
            <div class="flex h-9 w-10 shrink-0 items-center justify-center">
              <button
                type="button"
                aria-label="删除断言"
                class="rounded p-1 text-muted-foreground hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
                @click="draft.assertions.checks.splice(idx, 1)"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 6. Request-level TLS & Timeout -->
      <div v-else class="p-5 space-y-5">
        <div class="flex flex-wrap items-center justify-between gap-4">
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch v-model="draft.tls.insecure_skip_verify" />
            <span>跳过服务端 TLS 证书校验 (覆盖环境默认设置)</span>
          </label>

          <div class="flex items-center gap-2.5">
            <span class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              单请求独立超时 (秒)
            </span>
            <UiInput
              :model-value="draft.timeout_secs ?? ''"
              type="number"
              min="1"
              mono
              placeholder="跟随压测配置"
              class="w-36"
              @update:model-value="
                (v) => (draft.timeout_secs = v ? Math.max(1, Number(v)) : null)
              "
            />
          </div>
        </div>

        <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              自定义 CA PEM / 路径
            </label>
            <UiTextarea
              :model-value="draft.tls.ca_cert ?? ''"
              :rows="3"
              mono
              placeholder="留空则继承环境 TLS"
              @update:model-value="(v) => (draft.tls.ca_cert = v)"
            />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              mTLS 客户端证书
            </label>
            <UiTextarea
              :model-value="draft.tls.client_cert ?? ''"
              :rows="3"
              mono
              @update:model-value="(v) => (draft.tls.client_cert = v)"
            />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              mTLS 客户端私钥
            </label>
            <UiTextarea
              :model-value="draft.tls.client_key ?? ''"
              :rows="3"
              mono
              @update:model-value="(v) => (draft.tls.client_key = v)"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { Plus, Trash2 } from "lucide-vue-next";
import type { TenantSpec } from "../../api/types";
import { BODY_CONTENT_TYPES, createDefaultTenant } from "../../helpers/format";
import {
  activeEnvironmentDetail,
  activeEnvironmentId,
  activeTab,
  openEnvironmentEditor,
  type WorkspaceTab,
} from "../../stores/workspace";
import CodeMirrorEditor from "../common/CodeMirrorEditor.vue";
import KeyValueEditor from "../common/KeyValueEditor.vue";
import TenantCryptoForm from "../common/TenantCryptoForm.vue";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiSwitch from "../ui/UiSwitch.vue";
import UiTextarea from "../ui/UiTextarea.vue";

const draft = computed(() => activeTab.value.draft);

const activeParamsCount = computed(
  () => draft.value.params.filter((p) => p.active && p.key.trim()).length,
);
const activeHeadersCount = computed(
  () => draft.value.headers.filter((h) => h.active && h.key.trim()).length,
);

const subTabs = computed<
  {
    id: WorkspaceTab["requestSubTab"];
    label: string;
    count: number;
    dot: boolean;
  }[]
>(() => [
  {
    id: "params",
    label: "查询参数",
    count: activeParamsCount.value,
    dot: false,
  },
  {
    id: "body",
    label: "请求体",
    count: 0,
    dot: Boolean(draft.value.body.content_type),
  },
  {
    id: "headers",
    label: "请求头",
    count: activeHeadersCount.value,
    dot: false,
  },
  {
    id: "auth",
    label: draft.value.protocol === "mazbri" ? "Mazbri 协议与加密" : "鉴权认证",
    count: 0,
    dot:
      draft.value.protocol === "mazbri" ||
      (draft.value.auth.auth_type !== "none" &&
        draft.value.auth.auth_type !== "inherit"),
  },
  {
    id: "assertions",
    label: "响应断言",
    count: draft.value.assertions.checks.length,
    dot: false,
  },
  {
    id: "tls",
    label: "TLS / 超时",
    count: 0,
    dot:
      draft.value.tls.insecure_skip_verify ||
      Boolean(draft.value.tls.ca_cert || draft.value.tls.client_cert),
  },
]);

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
