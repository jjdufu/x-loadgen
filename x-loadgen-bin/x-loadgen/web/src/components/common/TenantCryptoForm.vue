<template>
  <div class="space-y-4">
    <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          租户标识名称 (Name) *
        </label>
        <input
          :value="tenant.name"
          type="text"
          placeholder="例如 tenant-gm-01"
          class="input font-mono"
          @input="patch({ name: ($event.target as HTMLInputElement).value })"
        />
      </div>
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          流量权重 (Weight)
        </label>
        <input
          :value="tenant.weight"
          type="number"
          min="1"
          class="input font-mono"
          @input="patch({ weight: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1) })"
        />
      </div>
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          换票 API Key (/auth/token) *
        </label>
        <div class="flex items-center space-x-1">
          <input
            :value="tenant.api_key ?? ''"
            :type="showApiKey ? 'text' : 'password'"
            placeholder="输入客户端 api_key (支持 <<var>>)"
            class="input font-mono flex-1"
            @input="patch({ api_key: ($event.target as HTMLInputElement).value })"
          />
          <HoppButtonSecondary
            v-tippy="showApiKey ? '隐藏密钥' : '显示密钥'"
            :icon="showApiKey ? EyeOff : Eye"
            outline
            @click="showApiKey = !showApiKey"
          />
        </div>
      </div>
    </div>

    <!-- Envelope Encryption Section -->
    <div class="p-3 rounded border border-dividerLight bg-primaryLight/40 space-y-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center space-x-2">
          <ShieldCheck class="svg-icons text-accent" />
          <span class="font-semibold text-secondaryDark">载荷信封加密 (Transport Envelope)</span>
        </div>
        <div class="flex items-center space-x-1 bg-primary rounded border border-divider p-0.5">
          <button
            v-for="m in encryptionModes"
            :key="m.value"
            type="button"
            class="px-2.5 py-1 rounded text-tiny font-semibold transition"
            :class="
              tenant.encryption_mode === m.value
                ? 'bg-accent text-accentContrast'
                : 'text-secondary hover:text-secondaryDark'
            "
            @click="setEncryptionMode(m.value)"
          >
            {{ m.label }}
          </button>
        </div>
      </div>

      <template v-if="tenant.encryption_mode === 'request' || tenant.encryption_mode === 'full'">
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              网关租户 ID (tenant_id) *
            </label>
            <input
              :value="tenant.tenant_id ?? ''"
              type="text"
              placeholder="例如 t-001 或 UUID"
              class="input font-mono"
              @input="patch({ tenant_id: ($event.target as HTMLInputElement).value })"
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              密钥版本 (key_version) *
            </label>
            <input
              :value="tenant.key_version ?? ''"
              type="text"
              placeholder="例如 v1"
              class="input font-mono"
              @input="patch({ key_version: ($event.target as HTMLInputElement).value })"
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              算法配置模式
            </label>
            <select
              :value="useCustomCipher ? 'custom' : 'suite'"
              class="select"
              @change="toggleCipherSource(($event.target as HTMLSelectElement).value)"
            >
              <option value="suite">命名密码套件 (Suite 推荐)</option>
              <option value="custom">自定义数据/MAC 算法组合</option>
            </select>
          </div>
        </div>

        <div v-if="!useCustomCipher">
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            命名套件 (Suite) *
          </label>
          <select
            :value="tenant.suite ?? 'suite/tenant-default-2026-11'"
            class="select font-mono"
            @change="
              patch({
                suite: ($event.target as HTMLSelectElement).value,
                data_alg: '',
                mac_alg: '',
              })
            "
          >
            <option v-for="s in MAZBRI_SUITES" :key="s.value" :value="s.value">
              {{ s.label }}
            </option>
          </select>
        </div>

        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              对称加密算法 (data_alg) *
            </label>
            <select
              :value="tenant.data_alg ?? 'aes-256-gcm'"
              class="select font-mono"
              @change="patch({ suite: '', data_alg: ($event.target as HTMLSelectElement).value })"
            >
              <option v-for="a in MAZBRI_DATA_ALGS" :key="a.value" :value="a.value">
                {{ a.label }}
              </option>
            </select>
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              外挂消息认证码 (mac_alg)
            </label>
            <select
              :value="tenant.mac_alg ?? ''"
              class="select font-mono"
              @change="patch({ suite: '', mac_alg: ($event.target as HTMLSelectElement).value })"
            >
              <option v-for="m in MAZBRI_MAC_ALGS" :key="m.value" :value="m.value">
                {{ m.label }}
              </option>
            </select>
          </div>
        </div>

        <div>
          <div class="flex items-center justify-between mb-1">
            <label class="text-tiny text-secondaryLight font-semibold">
              租户对称主密钥 Base64 (tenant_key_base64) *
            </label>
            <HoppButtonSecondary
              :icon="Dices"
              label="生成 32B Base64 密钥"
              @click="fillRandomBase64Key('tenant_key_base64', 32)"
            />
          </div>
          <div class="flex items-center space-x-1">
            <input
              :value="tenant.tenant_key_base64 ?? ''"
              :type="showTenantKey ? 'text' : 'password'"
              placeholder="标准 Base64 编码密钥"
              class="input font-mono flex-1"
              @input="patch({ tenant_key_base64: ($event.target as HTMLInputElement).value })"
            />
            <HoppButtonSecondary
              :icon="showTenantKey ? EyeOff : Eye"
              outline
              @click="showTenantKey = !showTenantKey"
            />
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-6 pt-1">
          <HoppSmartToggle
            :on="Boolean(tenant.allow_insecure)"
            @change="patch({ allow_insecure: !tenant.allow_insecure })"
          >
            允许 Legacy 算法 (ECB / DES / HMAC-MD5)
          </HoppSmartToggle>
          <HoppSmartToggle
            :on="Boolean(tenant.require_signature)"
            @change="patch({ require_signature: tenant.require_signature ? null : true })"
          >
            强制校验信封 MAC 签名 (require_signature)
          </HoppSmartToggle>
        </div>
      </template>
    </div>

    <!-- Request Binding Signature Section -->
    <div class="p-3 rounded border border-dividerLight bg-primaryLight/40 space-y-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center space-x-2">
          <KeyRound class="svg-icons text-accent" />
          <span class="font-semibold text-secondaryDark">
            请求绑定签名 (x-mazbri-* Request Binding)
          </span>
        </div>
        <HoppButtonSecondary
          v-if="asymmetricBinding"
          :icon="Sparkles"
          :loading="generatingKeypair"
          label="一键生成非对称密钥对"
          outline
          @click="generateKeypair"
        />
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            签名算法 (binding_alg)
          </label>
          <select
            :value="tenant.binding_alg ?? ''"
            class="select font-mono"
            @change="patch({ binding_alg: ($event.target as HTMLSelectElement).value })"
          >
            <option v-for="b in MAZBRI_BINDING_ALGS" :key="b.value" :value="b.value">
              {{ b.label }}
            </option>
          </select>
        </div>
        <div v-if="tenant.binding_alg">
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            签名密钥 ID (binding_kid -> x-mazbri-key-id) *
          </label>
          <input
            :value="tenant.binding_kid ?? ''"
            type="text"
            placeholder="例如 k1"
            class="input font-mono"
            @input="patch({ binding_kid: ($event.target as HTMLInputElement).value })"
          />
        </div>
      </div>

      <template v-if="tenant.binding_alg">
        <div v-if="!asymmetricBinding">
          <div class="flex items-center justify-between mb-1">
            <label class="text-tiny text-secondaryLight font-semibold">
              对称签名密钥 Base64 (binding_key) *
            </label>
            <HoppButtonSecondary
              :icon="Dices"
              label="随机生成 HMAC Base64"
              @click="fillRandomBase64Key('binding_key', 32)"
            />
          </div>
          <div class="flex items-center space-x-1">
            <input
              :value="tenant.binding_key ?? ''"
              :type="showBindingKey ? 'text' : 'password'"
              placeholder="标准 Base64 编码 HMAC 密钥"
              class="input font-mono flex-1"
              @input="patch({ binding_key: ($event.target as HTMLInputElement).value })"
            />
            <HoppButtonSecondary
              :icon="showBindingKey ? EyeOff : Eye"
              outline
              @click="showBindingKey = !showBindingKey"
            />
          </div>
        </div>

        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              私钥 PEM (binding_key — PKCS#8 PEM) *
            </label>
            <textarea
              :value="tenant.binding_key ?? ''"
              rows="5"
              placeholder="-----BEGIN PRIVATE KEY-----&#10;...&#10;-----END PRIVATE KEY-----"
              class="textarea font-mono text-tiny"
              @input="patch({ binding_key: ($event.target as HTMLTextAreaElement).value })"
            />
          </div>
          <div>
            <div class="flex items-center justify-between mb-1">
              <label class="text-tiny text-secondaryLight font-semibold">
                公钥 PEM (登记到 Mazbri 网关)
              </label>
              <HoppButtonSecondary
                v-if="tenant.binding_public_key"
                :icon="Copy"
                label="复制公钥"
                @click="copyPublicKey"
              />
            </div>
            <textarea
              :value="tenant.binding_public_key ?? ''"
              rows="5"
              readonly
              placeholder="点击右上角「一键生成非对称密钥对」后自动回填 SPKI 公钥 PEM"
              class="textarea font-mono text-tiny bg-primary"
            />
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { HoppButtonSecondary, HoppSmartToggle } from "@hoppscotch/ui";
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
