<template>
  <div class="space-y-4">
    <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          租户标识名称 (Name) *
        </label>
        <UiInput
          :model-value="tenant.name"
          mono
          placeholder="例如 tenant-gm-01"
          @update:model-value="(v) => patch({ name: v })"
        />
      </div>

      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          流量权重 (Weight)
        </label>
        <UiInput
          :model-value="tenant.weight"
          type="number"
          min="1"
          mono
          @update:model-value="(v) => patch({ weight: Math.max(1, Number(v) || 1) })"
        />
      </div>

      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          换票 API Key (/auth/token) *
        </label>
        <div class="flex items-center gap-1.5">
          <UiInput
            :model-value="tenant.api_key ?? ''"
            :type="showApiKey ? 'text' : 'password'"
            mono
            placeholder="客户端 api_key (支持 <<var>>)"
            class="flex-1"
            @update:model-value="(v) => patch({ api_key: v })"
          />
          <UiButton
            variant="outline"
            size="icon"
            :title="showApiKey ? '隐藏密钥' : '显示密钥'"
            @click="showApiKey = !showApiKey"
          >
            <EyeOff v-if="showApiKey" class="h-4 w-4" />
            <Eye v-else class="h-4 w-4" />
          </UiButton>
        </div>
      </div>
    </div>

    <!-- Envelope Encryption Section -->
    <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-2">
          <ShieldCheck class="h-4 w-4 text-foreground shrink-0" />
          <span class="text-sm font-semibold text-foreground">
            载荷信封加密 (Transport Envelope)
          </span>
        </div>

        <div class="inline-flex h-8 items-center rounded-md border border-border bg-muted p-0.5">
          <button
            v-for="m in encryptionModes"
            :key="m.value"
            type="button"
            class="h-full rounded px-2.5 text-xs font-medium whitespace-nowrap transition-all"
            :class="
              tenant.encryption_mode === m.value
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            "
            @click="setEncryptionMode(m.value)"
          >
            {{ m.label }}
          </button>
        </div>
      </div>

      <template v-if="tenant.encryption_mode === 'request' || tenant.encryption_mode === 'full'">
        <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              网关租户 ID (tenant_id) *
            </label>
            <UiInput
              :model-value="tenant.tenant_id ?? ''"
              mono
              placeholder="例如 t-001 或 UUID"
              @update:model-value="(v) => patch({ tenant_id: v })"
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              密钥版本 (key_version) *
            </label>
            <UiInput
              :model-value="tenant.key_version ?? ''"
              mono
              placeholder="例如 v1"
              @update:model-value="(v) => patch({ key_version: v })"
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              算法配置模式
            </label>
            <UiSelect
              :model-value="useCustomCipher ? 'custom' : 'suite'"
              @update:model-value="toggleCipherSource"
            >
              <option value="suite">命名密码套件 (Suite 推荐)</option>
              <option value="custom">自定义数据 / MAC 算法组合</option>
            </UiSelect>
          </div>
        </div>

        <div v-if="!useCustomCipher" class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            命名套件 (Suite) *
          </label>
          <UiSelect
            :model-value="tenant.suite ?? 'suite/tenant-default-2026-11'"
            mono
            @update:model-value="
              (v) =>
                patch({
                  suite: v,
                  data_alg: '',
                  mac_alg: '',
                })
            "
          >
            <option v-for="s in MAZBRI_SUITES" :key="s.value" :value="s.value">
              {{ s.label }}
            </option>
          </UiSelect>
        </div>

        <div v-else class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              对称加密算法 (data_alg) *
            </label>
            <UiSelect
              :model-value="tenant.data_alg ?? 'aes-256-gcm'"
              mono
              @update:model-value="(v) => patch({ suite: '', data_alg: v })"
            >
              <option v-for="a in MAZBRI_DATA_ALGS" :key="a.value" :value="a.value">
                {{ a.label }}
              </option>
            </UiSelect>
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              外挂消息认证码 (mac_alg)
            </label>
            <UiSelect
              :model-value="tenant.mac_alg ?? ''"
              mono
              @update:model-value="(v) => patch({ suite: '', mac_alg: v })"
            >
              <option v-for="m in MAZBRI_MAC_ALGS" :key="m.value" :value="m.value">
                {{ m.label }}
              </option>
            </UiSelect>
          </div>
        </div>

        <div class="space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              租户对称主密钥 Base64 (tenant_key_base64) *
            </label>
            <UiButton
              variant="ghost"
              size="xs"
              @click="fillRandomBase64Key('tenant_key_base64', 32)"
            >
              <Dices class="h-3.5 w-3.5" />
              <span>生成 32B Base64 密钥</span>
            </UiButton>
          </div>
          <div class="flex items-center gap-1.5">
            <UiInput
              :model-value="tenant.tenant_key_base64 ?? ''"
              :type="showTenantKey ? 'text' : 'password'"
              mono
              placeholder="标准 Base64 编码密钥"
              class="flex-1"
              @update:model-value="(v) => patch({ tenant_key_base64: v })"
            />
            <UiButton
              variant="outline"
              size="icon"
              @click="showTenantKey = !showTenantKey"
            >
              <EyeOff v-if="showTenantKey" class="h-4 w-4" />
              <Eye v-else class="h-4 w-4" />
            </UiButton>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-6 pt-1">
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch
              :model-value="Boolean(tenant.allow_insecure)"
              @update:model-value="(v) => patch({ allow_insecure: v })"
            />
            <span>允许 Legacy 算法 (ECB / DES / HMAC-MD5)</span>
          </label>
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch
              :model-value="Boolean(tenant.require_signature)"
              @update:model-value="(v) => patch({ require_signature: v ? true : null })"
            />
            <span>强制校验信封 MAC 签名 (require_signature)</span>
          </label>
        </div>
      </template>
    </div>

    <!-- Request Binding Signature Section -->
    <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <KeyRound class="h-4 w-4 text-foreground shrink-0" />
          <span class="text-sm font-semibold text-foreground">
            请求绑定签名 (x-mazbri-* Request Binding)
          </span>
        </div>
        <UiButton
          v-if="asymmetricBinding"
          variant="outline"
          size="sm"
          :loading="generatingKeypair"
          @click="generateKeypair"
        >
          <Sparkles class="h-3.5 w-3.5" />
          <span>一键生成非对称密钥对</span>
        </UiButton>
      </div>

      <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            签名算法 (binding_alg)
          </label>
          <UiSelect
            :model-value="tenant.binding_alg ?? ''"
            mono
            @update:model-value="(v) => patch({ binding_alg: v })"
          >
            <option v-for="b in MAZBRI_BINDING_ALGS" :key="b.value" :value="b.value">
              {{ b.label }}
            </option>
          </UiSelect>
        </div>

        <div v-if="tenant.binding_alg" class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            签名密钥 ID (binding_kid · x-mazbri-key-id) *
          </label>
          <UiInput
            :model-value="tenant.binding_kid ?? ''"
            mono
            placeholder="例如 k1"
            @update:model-value="(v) => patch({ binding_kid: v })"
          />
        </div>
      </div>

      <template v-if="tenant.binding_alg">
        <div v-if="!asymmetricBinding" class="space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              对称签名密钥 Base64 (binding_key) *
            </label>
            <UiButton
              variant="ghost"
              size="xs"
              @click="fillRandomBase64Key('binding_key', 32)"
            >
              <Dices class="h-3.5 w-3.5" />
              <span>随机生成 HMAC Base64</span>
            </UiButton>
          </div>
          <div class="flex items-center gap-1.5">
            <UiInput
              :model-value="tenant.binding_key ?? ''"
              :type="showBindingKey ? 'text' : 'password'"
              mono
              placeholder="标准 Base64 编码 HMAC 密钥"
              class="flex-1"
              @update:model-value="(v) => patch({ binding_key: v })"
            />
            <UiButton
              variant="outline"
              size="icon"
              @click="showBindingKey = !showBindingKey"
            >
              <EyeOff v-if="showBindingKey" class="h-4 w-4" />
              <Eye v-else class="h-4 w-4" />
            </UiButton>
          </div>
        </div>

        <div v-else class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              私钥 PEM (binding_key — PKCS#8 PEM) *
            </label>
            <UiTextarea
              :model-value="tenant.binding_key ?? ''"
              :rows="5"
              mono
              placeholder="-----BEGIN PRIVATE KEY-----&#10;...&#10;-----END PRIVATE KEY-----"
              @update:model-value="(v) => patch({ binding_key: v })"
            />
          </div>

          <div class="space-y-1.5">
            <div class="flex items-center justify-between gap-2">
              <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
                公钥 PEM (登记到 Mazbri 网关)
              </label>
              <UiButton
                v-if="tenant.binding_public_key"
                variant="ghost"
                size="xs"
                @click="copyPublicKey"
              >
                <Copy class="h-3.5 w-3.5" />
                <span>复制公钥</span>
              </UiButton>
            </div>
            <UiTextarea
              :model-value="tenant.binding_public_key ?? ''"
              :rows="5"
              readonly
              mono
              placeholder="点击「一键生成非对称密钥对」后自动回填 SPKI 公钥 PEM"
              class="bg-muted/30"
            />
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  Copy,
  Dices,
  Eye,
  EyeOff,
  KeyRound,
  ShieldCheck,
  Sparkles,
} from "lucide-vue-next";
import { api } from "../../api/workspace";
import type { TenantSpec } from "../../api/types";
import {
  MAZBRI_BINDING_ALGS,
  MAZBRI_DATA_ALGS,
  MAZBRI_MAC_ALGS,
  MAZBRI_SUITES,
  isAsymmetricBindingAlg,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiSwitch from "../ui/UiSwitch.vue";
import UiTextarea from "../ui/UiTextarea.vue";

const props = defineProps<{
  modelValue: TenantSpec;
  environmentId?: string | null;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: TenantSpec];
}>();

const showApiKey = ref(false);
const showTenantKey = ref(false);
const showBindingKey = ref(false);
const generatingKeypair = ref(false);

const encryptionModes = [
  { value: "none", label: "明文 (none)" },
  { value: "request", label: "仅请求加密 (request)" },
  { value: "full", label: "双向信封加密 (full)" },
] as const;

const tenant = computed(() => props.modelValue);

const useCustomCipher = ref(
  Boolean(!props.modelValue.suite && props.modelValue.data_alg),
);

const asymmetricBinding = computed(() =>
  isAsymmetricBindingAlg(tenant.value.binding_alg),
);

function patch(partial: Partial<TenantSpec>): void {
  emit("update:modelValue", { ...tenant.value, ...partial });
}

function setEncryptionMode(mode: string): void {
  if (mode === "none") {
    patch({ encryption_mode: "none" });
    return;
  }
  patch({
    encryption_mode: mode,
    key_version: tenant.value.key_version || "v1",
    suite: useCustomCipher.value
      ? ""
      : tenant.value.suite || "suite/tenant-default-2026-11",
    data_alg: useCustomCipher.value
      ? tenant.value.data_alg || "aes-256-gcm"
      : "",
  });
}

function toggleCipherSource(mode: string): void {
  if (mode === "custom") {
    useCustomCipher.value = true;
    patch({
      suite: "",
      data_alg: tenant.value.data_alg || "aes-256-gcm",
    });
  } else {
    useCustomCipher.value = false;
    patch({
      suite: tenant.value.suite || "suite/tenant-default-2026-11",
      data_alg: "",
      mac_alg: "",
    });
  }
}

function fillRandomBase64Key(
  field: "tenant_key_base64" | "binding_key",
  byteLen: number,
): void {
  const bytes = new Uint8Array(byteLen);
  window.crypto.getRandomValues(bytes);
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  patch({ [field]: window.btoa(binary) });
  toast.info("已生成随机 Base64 密钥");
}

async function generateKeypair(): Promise<void> {
  const alg = tenant.value.binding_alg?.trim();
  if (!alg) return;
  const kid = tenant.value.binding_kid?.trim() || "k1";
  generatingKeypair.value = true;
  try {
    if (props.environmentId && tenant.value.id) {
      try {
        const res = await api.generateTenantKeypair(
          props.environmentId,
          tenant.value.id,
          alg,
          kid,
        );
        patch({
          binding_kid: res.binding_kid,
          binding_alg: res.binding_alg,
          binding_public_key: res.public_key_pem,
          ...(res.private_key_pem ? { binding_key: res.private_key_pem } : {}),
        });
        toast.success(`已为租户生成并持久化 ${alg} 密钥对`);
        return;
      } catch {
        // Fall through to stateless generation if tenant row isn't saved yet
      }
    }
    const res = await api.generateStatelessKeypair(alg, kid);
    patch({
      binding_kid: res.binding_kid,
      binding_alg: res.binding_alg,
      binding_key: res.private_key_pem ?? "",
      binding_public_key: res.public_key_pem,
    });
    toast.success(`已生成 ${alg} 密钥对（私钥已填入表单）`);
  } finally {
    generatingKeypair.value = false;
  }
}

async function copyPublicKey(): Promise<void> {
  if (!tenant.value.binding_public_key) return;
  try {
    await navigator.clipboard.writeText(tenant.value.binding_public_key);
    toast.success("公钥 PEM 已复制");
  } catch {
    toast.error("复制失败");
  }
}
</script>
