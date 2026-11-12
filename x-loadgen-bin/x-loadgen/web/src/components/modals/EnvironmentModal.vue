<template>
  <UiDialog
    :open="envModalOpen"
    :title="modalTitle"
    description="管理运行环境变量 (<<var>>)、默认网关地址、Mazbri 租户密钥池及默认 TLS/认证配置。"
    max-width="4xl"
    @close="close"
  >
    <div v-if="loading" class="flex items-center justify-center py-14">
      <Loader2 class="h-6 w-6 animate-spin text-foreground" />
    </div>

    <div v-else class="space-y-5">
      <div v-if="!isGlobal" class="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            环境名称 *
          </label>
          <UiInput
            v-model="form.name"
            placeholder="例如 测试环境 / 模拟网关"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            默认网关地址（供相对路径与 Mazbri 使用）
          </label>
          <UiInput
            v-model="gatewayUrl"
            mono
            placeholder="例如 http://127.0.0.1:18080"
          />
        </div>
      </div>

      <!-- Shadcn Segmented Sub-Tabs -->
      <div class="inline-flex flex-wrap items-center rounded-lg border border-border bg-muted p-1 gap-0.5">
        <button
          v-for="tab in modalTabs"
          :key="tab.id"
          type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-xs font-medium whitespace-nowrap transition-all"
          :class="
            activeSubTab === tab.id
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          "
          @click="activeSubTab = tab.id"
        >
          <span>{{ tab.label }}</span>
          <span
            v-if="tab.count"
            class="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-foreground tabular-nums"
          >
            {{ tab.count }}
          </span>
        </button>
      </div>

      <!-- 1. Variables -->
      <div v-if="activeSubTab === 'vars'" class="space-y-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <p class="text-xs text-muted-foreground leading-relaxed">
            在地址、参数、请求头、请求体或密钥中使用
            <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">&lt;&lt;变量名&gt;&gt;</code>
            引用；同时内置
            <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">&#123;&#123;seq&#125;&#125;</code> /
            <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">&#123;&#123;uuid&#125;&#125;</code> /
            <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">&#123;&#123;timestamp_ms&#125;&#125;</code>。
          </p>
          <UiButton variant="outline" size="sm" @click="addVar">
            <Plus class="h-3.5 w-3.5" />
            <span>添加变量</span>
          </UiButton>
        </div>

        <div
          v-if="form.variables.length === 0"
          class="rounded-lg border border-dashed border-border py-8 text-center text-sm text-muted-foreground"
        >
          暂无环境变量，点击右上角「添加变量」创建
        </div>

        <div v-else class="divide-y divide-border overflow-hidden rounded-lg border border-border">
          <div
            v-for="(v, idx) in form.variables"
            :key="idx"
            class="flex items-center divide-x divide-border bg-background"
          >
            <div class="flex h-9 w-10 shrink-0 items-center justify-center">
              <input
                type="checkbox"
                :checked="v.active"
                :aria-label="`启用变量 ${idx + 1}`"
                class="h-4 w-4 cursor-pointer rounded border-input accent-foreground"
                @change="v.active = !v.active"
              />
            </div>
            <div class="min-w-0 flex-1">
              <input
                v-model="v.key"
                type="text"
                placeholder="变量名 (例如 baseUrl)"
                class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
            </div>
            <div class="min-w-0 flex-1">
              <input
                v-model="v.value"
                :type="v.secret ? 'password' : 'text'"
                placeholder="变量值"
                class="h-9 w-full bg-transparent px-3 font-mono text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
            </div>
            <div class="flex h-9 shrink-0 items-center gap-0.5 px-2">
              <button
                type="button"
                :title="v.secret ? '敏感变量（导出默认脱敏）' : '标记为敏感变量'"
                class="rounded p-1.5 transition-colors hover:bg-accent"
                :class="v.secret ? 'text-amber-600 dark:text-amber-400' : 'text-muted-foreground'"
                @click="v.secret = !v.secret"
              >
                <Lock v-if="v.secret" class="h-3.5 w-3.5" />
                <Unlock v-else class="h-3.5 w-3.5" />
              </button>
              <button
                type="button"
                title="删除变量"
                class="rounded p-1.5 text-muted-foreground transition-colors hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
                @click="form.variables.splice(idx, 1)"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 2. Mazbri Tenant Pool -->
      <div v-else-if="activeSubTab === 'tenants' && !isGlobal" class="space-y-4">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <p class="text-xs text-muted-foreground leading-relaxed max-w-2xl">
            当 Mazbri 请求选择「环境租户池」时，压测引擎将按权重自动轮询下方租户执行换票、签名与信封加解密。
          </p>
          <UiButton variant="outline" size="sm" @click="addTenant">
            <Plus class="h-3.5 w-3.5" />
            <span>新增租户</span>
          </UiButton>
        </div>

        <div
          v-if="tenants.length === 0"
          class="rounded-lg border border-dashed border-border py-10 text-center space-y-3"
        >
          <p class="text-sm text-muted-foreground">
            尚未配置 Mazbri 租户（纯 HTTP 请求无需配置）
          </p>
          <UiButton variant="outline" size="sm" @click="addTenant">
            <Plus class="h-3.5 w-3.5" />
            <span>新增租户</span>
          </UiButton>
        </div>

        <div v-else class="space-y-4">
          <div class="flex flex-wrap items-center gap-2 border-b border-border pb-3">
            <button
              v-for="(t, idx) in tenants"
              :key="idx"
              type="button"
              class="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 font-mono text-xs transition-colors"
              :class="
                selectedTenantIdx === idx
                  ? 'border-foreground bg-primary text-primary-foreground font-medium'
                  : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'
              "
              @click="selectedTenantIdx = idx"
            >
              <span>{{ t.name || `tenant-${idx + 1}` }}</span>
              <span class="opacity-70">权重 {{ t.weight }}</span>
            </button>
          </div>

          <div v-if="tenants[selectedTenantIdx]" class="space-y-3">
            <div class="flex justify-end">
              <UiButton
                variant="outline"
                size="xs"
                class="text-rose-600 dark:text-rose-400 hover:text-rose-600"
                @click="removeTenant(selectedTenantIdx)"
              >
                <Trash2 class="h-3.5 w-3.5" />
                <span>移除租户</span>
              </UiButton>
            </div>
            <TenantCryptoForm
              :model-value="tenants[selectedTenantIdx]!"
              :environment-id="editingEnvId"
              @update:model-value="(val) => (tenants[selectedTenantIdx] = val)"
            />
          </div>
        </div>
      </div>

      <!-- 3. Default HTTP Auth -->
      <div v-else-if="activeSubTab === 'auth' && !isGlobal" class="space-y-4">
        <div class="max-w-sm space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            默认认证方式（请求选择「继承默认」时生效）
          </label>
          <UiSelect v-model="httpAuth.auth_type">
            <option value="none">无认证</option>
            <option value="bearer">Bearer Token</option>
            <option value="basic">Basic Auth</option>
            <option value="api-key">API Key</option>
          </UiSelect>
        </div>

        <div v-if="httpAuth.auth_type === 'bearer'" class="max-w-xl space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            Bearer Token
          </label>
          <UiInput v-model="httpAuthToken" mono placeholder="输入 Token 或 <<token>>" />
        </div>

        <div v-else-if="httpAuth.auth_type === 'basic'" class="grid max-w-xl grid-cols-2 gap-4">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              用户名
            </label>
            <UiInput v-model="httpAuthUser" mono />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              密码
            </label>
            <UiInput v-model="httpAuthPass" type="password" mono />
          </div>
        </div>

        <div v-else-if="httpAuth.auth_type === 'api-key'" class="grid max-w-2xl grid-cols-3 gap-4">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              参数名
            </label>
            <UiInput v-model="httpAuthKey" mono placeholder="X-API-Key" />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              参数值
            </label>
            <UiInput v-model="httpAuthVal" type="password" mono />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              注入位置
            </label>
            <UiSelect v-model="httpAuthAddTo">
              <option value="HEADERS">请求头</option>
              <option value="QUERY_PARAMS">查询参数</option>
            </UiSelect>
          </div>
        </div>
      </div>

      <!-- 4. TLS / mTLS -->
      <div v-else-if="activeSubTab === 'tls' && !isGlobal" class="space-y-4">
        <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
          <UiSwitch v-model="tls.insecure_skip_verify" />
          <span>跳过服务端 TLS 证书校验</span>
        </label>

        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            自定义根证书 CA PEM 或路径
          </label>
          <UiTextarea
            v-model="caCert"
            :rows="3"
            mono
            placeholder="-----BEGIN CERTIFICATE----- 或 /path/to/ca.pem"
          />
        </div>

        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              客户端证书 (mTLS)
            </label>
            <UiTextarea
              v-model="clientCert"
              :rows="3"
              mono
              placeholder="-----BEGIN CERTIFICATE-----"
            />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              客户端私钥 (mTLS)
            </label>
            <UiTextarea
              v-model="clientKey"
              :rows="3"
              mono
              placeholder="-----BEGIN PRIVATE KEY-----"
            />
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="flex w-full items-center justify-between gap-2">
        <div>
          <UiButton
            v-if="editingEnvId && !isGlobal"
            variant="outline"
            class="text-rose-600 dark:text-rose-400 hover:text-rose-600"
            @click="handleDelete"
          >
            <Trash2 class="h-4 w-4" />
            <span>删除环境</span>
          </UiButton>
        </div>
        <div class="flex items-center gap-2">
          <UiButton variant="outline" @click="close">取消</UiButton>
          <UiButton variant="default" :loading="saving" @click="handleSave">
            保存
          </UiButton>
        </div>
      </div>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Loader2, Lock, Plus, Trash2, Unlock } from "lucide-vue-next";
import { api } from "../../api/workspace";
import type {
  EnvVarRow,
  HttpAuthSpec,
  TenantSpec,
  TlsSpec,
} from "../../api/types";
import {
  createDefaultHttpAuth,
  createDefaultTenant,
  createDefaultTls,
  normalizeTenantForApi,
} from "../../helpers/format";
import { askConfirm } from "../../modules/confirm";
import { toast } from "../../modules/toast";
import {
  activeEnvironmentId,
  editingEnvId,
  envModalOpen,
  refreshEnvironments,
} from "../../stores/workspace";
import TenantCryptoForm from "../common/TenantCryptoForm.vue";
import UiButton from "../ui/UiButton.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiSwitch from "../ui/UiSwitch.vue";
import UiTextarea from "../ui/UiTextarea.vue";

const loading = ref(false);
const saving = ref(false);
const isGlobal = ref(false);
const activeSubTab = ref<"vars" | "tenants" | "auth" | "tls">("vars");
const selectedTenantIdx = ref(0);

const form = ref<{ name: string; variables: EnvVarRow[] }>({
  name: "",
  variables: [],
});
const gatewayUrl = ref("");
const httpAuth = ref<HttpAuthSpec>(createDefaultHttpAuth("none"));
const tls = ref<TlsSpec>(createDefaultTls());
const tenants = ref<TenantSpec[]>([]);

const modalTabs = computed(() => {
  const base: {
    id: "vars" | "tenants" | "auth" | "tls";
    label: string;
    count: number;
  }[] = [
    {
      id: "vars",
      label: "环境变量",
      count: form.value.variables.length,
    },
  ];
  if (!isGlobal.value) {
    base.push(
      {
        id: "tenants",
        label: "Mazbri 租户池",
        count: tenants.value.length,
      },
      { id: "auth", label: "默认认证", count: 0 },
      { id: "tls", label: "TLS / mTLS", count: 0 },
    );
  }
  return base;
});

const httpAuthToken = computed({
  get: () => httpAuth.value.token ?? "",
  set: (v: string) => (httpAuth.value.token = v),
});
const httpAuthUser = computed({
  get: () => httpAuth.value.username ?? "",
  set: (v: string) => (httpAuth.value.username = v),
});
const httpAuthPass = computed({
  get: () => httpAuth.value.password ?? "",
  set: (v: string) => (httpAuth.value.password = v),
});
const httpAuthKey = computed({
  get: () => httpAuth.value.key ?? "",
  set: (v: string) => (httpAuth.value.key = v),
});
const httpAuthVal = computed({
  get: () => httpAuth.value.value ?? "",
  set: (v: string) => (httpAuth.value.value = v),
});
const httpAuthAddTo = computed({
  get: () => httpAuth.value.add_to ?? "HEADERS",
  set: (v: string) => (httpAuth.value.add_to = v),
});

const caCert = computed({
  get: () => tls.value.ca_cert ?? "",
  set: (v: string) => (tls.value.ca_cert = v),
});
const clientCert = computed({
  get: () => tls.value.client_cert ?? "",
  set: (v: string) => (tls.value.client_cert = v),
});
const clientKey = computed({
  get: () => tls.value.client_key ?? "",
  set: (v: string) => (tls.value.client_key = v),
});

const modalTitle = computed(() => {
  if (isGlobal.value) return "编辑全局变量";
  return editingEnvId.value
    ? `编辑环境：${form.value.name || "未命名"}`
    : "新建环境";
});

watch(
  envModalOpen,
  async (open) => {
    if (!open) return;
    activeSubTab.value = "vars";
    selectedTenantIdx.value = 0;
    if (!editingEnvId.value) {
      isGlobal.value = false;
      form.value = { name: "", variables: [] };
      gatewayUrl.value = "";
      httpAuth.value = createDefaultHttpAuth("none");
      tls.value = createDefaultTls();
      tenants.value = [];
      return;
    }
    loading.value = true;
    try {
      const detail = await api.getEnvironment(editingEnvId.value);
      isGlobal.value = detail.is_global;
      form.value = {
        name: detail.name,
        variables: detail.variables.map((v) => ({ ...v })),
      };
      gatewayUrl.value = detail.gateway_base_url ?? "";
      httpAuth.value = { ...createDefaultHttpAuth("none"), ...detail.http_auth };
      tls.value = { ...createDefaultTls(), ...detail.tls };
      tenants.value = detail.tenants.map((t) => ({ ...t }));
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

function addVar(): void {
  form.value.variables.push({ key: "", value: "", secret: false, active: true });
}

function addTenant(): void {
  const idx = tenants.value.length + 1;
  tenants.value.push(createDefaultTenant(`tenant-${idx}`));
  selectedTenantIdx.value = tenants.value.length - 1;
}

function removeTenant(idx: number): void {
  tenants.value.splice(idx, 1);
  if (selectedTenantIdx.value >= tenants.value.length) {
    selectedTenantIdx.value = Math.max(0, tenants.value.length - 1);
  }
}

async function handleSave(): Promise<void> {
  if (!isGlobal.value && !form.value.name.trim()) {
    toast.error("请输入环境名称");
    return;
  }
  saving.value = true;
  try {
    const payload = {
      name: isGlobal.value ? "Global" : form.value.name.trim(),
      gateway_base_url: isGlobal.value ? null : gatewayUrl.value.trim() || null,
      variables: form.value.variables.filter((v) => v.key.trim() !== ""),
      http_auth: isGlobal.value ? createDefaultHttpAuth("none") : httpAuth.value,
      tls: isGlobal.value
        ? createDefaultTls()
        : {
            ca_cert: tls.value.ca_cert?.trim() || null,
            client_cert: tls.value.client_cert?.trim() || null,
            client_key: tls.value.client_key?.trim() || null,
            insecure_skip_verify: Boolean(tls.value.insecure_skip_verify),
          },
      tenants: isGlobal.value
        ? []
        : tenants.value.map((t) => normalizeTenantForApi(t)),
    };

    if (editingEnvId.value) {
      await api.updateEnvironment(editingEnvId.value, payload);
      toast.success(`环境「${isGlobal.value ? "全局变量" : payload.name}」已更新`);
    } else {
      const created = await api.createEnvironment(payload);
      activeEnvironmentId.value = created.id;
      toast.success(`环境「${created.name}」已创建`);
    }
    await refreshEnvironments();
    close();
  } finally {
    saving.value = false;
  }
}

async function handleDelete(): Promise<void> {
  if (!editingEnvId.value || isGlobal.value) return;
  const confirmed = await askConfirm({
    title: "删除环境",
    description: `确定删除环境「${form.value.name}」吗？`,
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  await api.deleteEnvironment(editingEnvId.value);
  if (activeEnvironmentId.value === editingEnvId.value) {
    activeEnvironmentId.value = null;
  }
  await refreshEnvironments();
  toast.success("环境已删除");
  close();
}

function close(): void {
  envModalOpen.value = false;
}
</script>
