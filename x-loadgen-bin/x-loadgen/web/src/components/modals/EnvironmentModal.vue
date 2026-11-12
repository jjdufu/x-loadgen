<template>
  <HoppSmartModal
    v-if="envModalOpen"
    dialog
    :title="modalTitle"
    styles="sm:max-w-4xl"
    @close="close"
  >
    <template #body>
      <div v-if="loading" class="flex items-center justify-center py-12">
        <HoppSmartSpinner />
      </div>
      <div v-else class="space-y-4">
        <div v-if="!isGlobal" class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              环境名称 (Environment Name) *
            </label>
            <input
              v-model="form.name"
              type="text"
              placeholder="例如 Staging / Mock Mazbri 网关"
              class="input"
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              默认网关地址 (gateway_base_url，供相对路径与 Mazbri 使用)
            </label>
            <input
              v-model="gatewayUrl"
              type="text"
              placeholder="例如 http://127.0.0.1:18080 或 https://api.example.com"
              class="input font-mono"
            />
          </div>
        </div>

        <HoppSmartTabs v-model="activeSubTab" render-inactive-tabs>
          <HoppSmartTab
            id="vars"
            label="环境变量 (<<var>>)"
            :info="form.variables.length ? String(form.variables.length) : null"
          >
            <div class="p-3 space-y-3">
              <div class="flex items-center justify-between">
                <p class="text-tiny text-secondaryLight">
                  在 URL、参数、请求头、Body 或密钥中使用
                  <code class="font-mono text-accent">&lt;&lt;变量名&gt;&gt;</code> 引用；压测支持动态内置变量
                  <code class="font-mono text-accent">&#123;&#123;seq&#125;&#125;</code> /
                  <code class="font-mono text-accent">&#123;&#123;uuid&#125;&#125;</code> /
                  <code class="font-mono text-accent">&#123;&#123;timestamp_ms&#125;&#125;</code>。
                </p>
                <HoppButtonSecondary :icon="Plus" label="添加变量" outline @click="addVar" />
              </div>

              <div v-if="form.variables.length === 0" class="py-6 text-center text-secondaryLight text-tiny">
                暂无环境变量，点击右上角「添加变量」创建
              </div>

              <div v-else class="divide-y divide-dividerLight border border-dividerLight rounded">
                <div
                  v-for="(v, idx) in form.variables"
                  :key="idx"
                  class="flex items-center divide-x divide-dividerLight"
                >
                  <div class="w-9 flex items-center justify-center py-1.5">
                    <HoppSmartCheckbox
                      :on="v.active"
                      :name="`env-var-${idx}`"
                      @change="v.active = !v.active"
                    />
                  </div>
                  <div class="flex-1">
                    <input
                      v-model="v.key"
                      type="text"
                      placeholder="变量名 (例如 baseUrl)"
                      class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark"
                    />
                  </div>
                  <div class="flex-1">
                    <input
                      v-model="v.value"
                      :type="v.secret ? 'password' : 'text'"
                      placeholder="变量值"
                      class="w-full px-3 py-1.5 bg-transparent font-mono text-secondaryDark"
                    />
                  </div>
                  <div class="px-2 flex items-center space-x-1">
                    <HoppButtonSecondary
                      v-tippy="v.secret ? '当前为敏感脱敏变量（导出默认隐藏）' : '标记为敏感变量'"
                      :icon="v.secret ? Lock : Unlock"
                      :class="{ '!text-amber-500': v.secret }"
                      @click="v.secret = !v.secret"
                    />
                    <HoppButtonSecondary
                      v-tippy="'删除变量'"
                      :icon="Trash"
                      color="red"
                      @click="form.variables.splice(idx, 1)"
                    />
                  </div>
                </div>
              </div>
            </div>
          </HoppSmartTab>

          <HoppSmartTab
            v-if="!isGlobal"
            id="tenants"
            label="Mazbri 租户密钥池"
            :info="tenants.length ? String(tenants.length) : null"
          >
            <div class="p-3 space-y-3">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <p class="text-tiny text-secondaryLight">
                  当请求协议选择 <code class="font-mono text-accent">mazbri</code> 且租户来源为「环境加权池」时，压测引擎按权重自动轮询下方租户完成换 Token、请求签名与信封加解密。
                </p>
                <HoppButtonSecondary
                  :icon="Plus"
                  label="新增 Mazbri 租户"
                  outline
                  @click="addTenant"
                />
              </div>

              <div v-if="tenants.length === 0" class="py-8 text-center border border-dashed border-divider rounded">
                <p class="text-secondaryLight text-tiny mb-3">
                  当前环境尚未配置 Mazbri 租户（纯 HTTP 压测无需配置）
                </p>
                <HoppButtonSecondary :icon="Plus" label="添加首个 Mazbri 租户" outline @click="addTenant" />
              </div>

              <div v-else class="space-y-3">
                <div class="flex flex-wrap items-center gap-1.5 border-b border-dividerLight pb-2">
                  <button
                    v-for="(t, idx) in tenants"
                    :key="idx"
                    type="button"
                    class="px-3 py-1 rounded text-tiny font-mono border transition flex items-center space-x-1.5"
                    :class="
                      selectedTenantIdx === idx
                        ? 'border-accent bg-accent/15 text-secondaryDark font-semibold'
                        : 'border-dividerLight text-secondary hover:text-secondaryDark'
                    "
                    @click="selectedTenantIdx = idx"
                  >
                    <span>{{ t.name || `tenant-${idx + 1}` }}</span>
                    <span class="opacity-60">w:{{ t.weight }}</span>
                  </button>
                </div>

                <div v-if="tenants[selectedTenantIdx]" class="space-y-3">
                  <div class="flex justify-end">
                    <HoppButtonSecondary
                      :icon="Trash2"
                      label="移除当前租户"
                      color="red"
                      outline
                      @click="removeTenant(selectedTenantIdx)"
                    />
                  </div>
                  <TenantCryptoForm
                    :model-value="tenants[selectedTenantIdx]!"
                    :environment-id="editingEnvId"
                    @update:model-value="(val) => (tenants[selectedTenantIdx] = val)"
                  />
                </div>
              </div>
            </div>
          </HoppSmartTab>

          <HoppSmartTab v-if="!isGlobal" id="auth" label="默认 HTTP 认证">
            <div class="p-3 space-y-3">
              <div>
                <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                  默认 HTTP 鉴权方式（当请求鉴权设为 Inherit 时自动继承）
                </label>
                <select v-model="httpAuth.auth_type" class="select max-w-xs">
                  <option value="none">无认证 (None)</option>
                  <option value="bearer">Bearer Token</option>
                  <option value="basic">Basic Auth</option>
                  <option value="api-key">API Key</option>
                </select>
              </div>
              <div v-if="httpAuth.auth_type === 'bearer'">
                <label class="block text-tiny text-secondaryLight mb-1">Bearer Token</label>
                <input v-model="httpAuthToken" type="text" class="input font-mono" placeholder="输入 Token 或 <<token>>" />
              </div>
              <div v-else-if="httpAuth.auth_type === 'basic'" class="grid grid-cols-2 gap-3">
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1">用户名</label>
                  <input v-model="httpAuthUser" type="text" class="input font-mono" />
                </div>
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1">密码</label>
                  <input v-model="httpAuthPass" type="password" class="input font-mono" />
                </div>
              </div>
              <div v-else-if="httpAuth.auth_type === 'api-key'" class="grid grid-cols-3 gap-3">
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1">参数名 (Key)</label>
                  <input v-model="httpAuthKey" type="text" class="input font-mono" placeholder="X-API-Key" />
                </div>
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1">参数值 (Value)</label>
                  <input v-model="httpAuthVal" type="password" class="input font-mono" />
                </div>
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1">注入位置</label>
                  <select v-model="httpAuthAddTo" class="select">
                    <option value="HEADERS">请求头 (Headers)</option>
                    <option value="QUERY_PARAMS">查询参数 (Query)</option>
                  </select>
                </div>
              </div>
            </div>
          </HoppSmartTab>

          <HoppSmartTab v-if="!isGlobal" id="tls" label="TLS / mTLS">
            <div class="p-3 space-y-3">
              <HoppSmartToggle
                :on="tls.insecure_skip_verify"
                @change="tls.insecure_skip_verify = !tls.insecure_skip_verify"
              >
                跳过服务端 TLS 证书校验 (insecure_skip_verify)
              </HoppSmartToggle>
              <div>
                <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                  自定义根证书 CA PEM 或服务端文件路径 (ca_cert)
                </label>
                <textarea v-model="caCert" rows="3" class="textarea font-mono text-tiny" placeholder="-----BEGIN CERTIFICATE----- 或 /path/to/ca.pem" />
              </div>
              <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                    双向认证客户端证书 (client_cert)
                  </label>
                  <textarea v-model="clientCert" rows="3" class="textarea font-mono text-tiny" placeholder="-----BEGIN CERTIFICATE-----" />
                </div>
                <div>
                  <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                    双向认证客户端私钥 (client_key)
                  </label>
                  <textarea v-model="clientKey" rows="3" class="textarea font-mono text-tiny" placeholder="-----BEGIN PRIVATE KEY-----" />
                </div>
              </div>
            </div>
          </HoppSmartTab>
        </HoppSmartTabs>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-between w-full">
        <div>
          <HoppButtonSecondary
            v-if="editingEnvId && !isGlobal"
            :icon="Trash2"
            label="删除此环境"
            color="red"
            outline
            @click="handleDelete"
          />
        </div>
        <div class="flex items-center space-x-2">
          <HoppButtonSecondary label="取消" outline @click="close" />
          <HoppButtonPrimary
            :loading="saving"
            label="保存环境配置"
            @click="handleSave"
          />
        </div>
      </div>
    </template>
  </HoppSmartModal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartCheckbox,
  HoppSmartModal,
  HoppSmartSpinner,
  HoppSmartTab,
  HoppSmartTabs,
  HoppSmartToggle,
} from "@hoppscotch/ui";
import { Lock, Plus, Trash, Trash2, Unlock } from "lucide-vue-next";
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
import { toast } from "../../modules/toast";
import {
  activeEnvironmentId,
  editingEnvId,
  envModalOpen,
  refreshEnvironments,
} from "../../stores/workspace";
import TenantCryptoForm from "../common/TenantCryptoForm.vue";

const loading = ref(false);
const saving = ref(false);
const isGlobal = ref(false);
const activeSubTab = ref("vars");
const selectedTenantIdx = ref(0);

const form = ref<{ name: string; variables: EnvVarRow[] }>({
  name: "",
  variables: [],
});
const gatewayUrl = ref("");
const httpAuth = ref<HttpAuthSpec>(createDefaultHttpAuth("none"));
const tls = ref<TlsSpec>(createDefaultTls());
const tenants = ref<TenantSpec[]>([]);

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
  if (isGlobal.value) return "编辑全局变量 (Global Variables)";
  return editingEnvId.value ? `编辑环境：${form.value.name}` : "新建环境与密钥池";
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
      toast.success(`环境「${payload.name}」已更新`);
    } else {
      const created = await api.createEnvironment(payload);
      activeEnvironmentId.value = created.id;
      toast.success(`环境「${created.name}」已创建并激活`);
    }
    await refreshEnvironments();
    close();
  } finally {
    saving.value = false;
  }
}

async function handleDelete(): Promise<void> {
  if (!editingEnvId.value || isGlobal.value) return;
  if (!confirm(`确定删除环境「${form.value.name}」吗？`)) return;
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
