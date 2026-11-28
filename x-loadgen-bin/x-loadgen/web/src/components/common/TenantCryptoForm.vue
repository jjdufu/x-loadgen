<template>
  <div class="space-y-4">
    <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          租户名称 *
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
          流量权重
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
          换票 API Key *
        </label>
        <div class="flex items-center gap-1.5">
          <UiInput
            :model-value="tenant.api_key ?? ''"
            :type="showApiKey ? 'text' : 'password'"
            mono
            placeholder="支持 <<var>>"
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
            信封加密
          </span>
        </div>

        <div class="inline-flex h-8 items-center rounded-md border border-border bg-muted p-0.5">
          <button
            v-for="m in encryptionModes"
            :key="m.value"
            type="button"
            class="h-full cursor-pointer rounded-md px-2.5 text-xs font-medium whitespace-nowrap transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
            :class="
              tenant.encryption_mode === m.value
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'
            "
            @click="setEncryptionMode(m.value)"
          >
            {{ m.label }}
          </button>
        </div>
      </div>

      <template v-if="tenant.encryption_mode === 'request' || tenant.encryption_mode === 'full'">
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              网关租户 ID *
            </label>
            <UiInput
              :model-value="tenant.tenant_id ?? ''"
              mono
              placeholder="例如 t-001"
              @update:model-value="(v) => patch({ tenant_id: v })"
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              算法配置
            </label>
            <UiSelect
              :model-value="useCustomCipher ? 'custom' : 'suite'"
              @update:model-value="toggleCipherSource"
            >
              <option value="suite">命名密码套件</option>
              <option value="custom">自定义算法组合</option>
            </UiSelect>
          </div>
        </div>

        <div v-if="!useCustomCipher" class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            密码套件 *
          </label>
          <UiSelect
            :model-value="tenant.suite ?? 'suite/aes-256-gcm'"
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
              对称加密算法 *
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
              外挂消息认证码
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
          <div class="flex h-5 items-center justify-between gap-2">
            <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              对称主密钥 (Base64) *
            </label>
            <button
              type="button"
              class="inline-flex h-5 cursor-pointer items-center gap-1 rounded-md px-1.5 text-xs font-medium text-muted-foreground transition-colors duration-100 hover:bg-hover-bg hover:text-foreground active:scale-[0.98] active:bg-active-bg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              @click="fillRandomBase64Key(32)"
            >
              <Dices class="h-3.5 w-3.5" />
              <span>随机生成</span>
            </button>
          </div>
          <div class="flex items-center gap-1.5">
            <UiInput
              :model-value="tenant.tenant_key_base64 ?? ''"
              :type="showTenantKey ? 'text' : 'password'"
              mono
              placeholder="Base64 编码密钥"
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
            <span>允许兼容算法 (ECB / DES / HMAC-MD5)</span>
          </label>
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch
              :model-value="Boolean(tenant.require_signature)"
              @update:model-value="(v) => patch({ require_signature: v ? true : null })"
            />
            <span>强制校验信封签名</span>
          </label>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { Dices, Eye, EyeOff, ShieldCheck } from "lucide-vue-next";
import type { TenantSpec } from "../../api/types";
import {
  MAZBRI_DATA_ALGS,
  MAZBRI_MAC_ALGS,
  MAZBRI_SUITES,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiSwitch from "../ui/UiSwitch.vue";

const props = defineProps<{
  modelValue: TenantSpec;
  environmentId?: string | null;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: TenantSpec];
}>();

const showApiKey = ref(false);
const showTenantKey = ref(false);

const encryptionModes = [
  { value: "none", label: "明文" },
  { value: "request", label: "仅请求加密" },
  { value: "full", label: "双向加密" },
] as const;

const tenant = computed(() => props.modelValue);

const useCustomCipher = ref(
  Boolean(!props.modelValue.suite && props.modelValue.data_alg),
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
    suite: useCustomCipher.value
      ? ""
      : tenant.value.suite || "suite/aes-256-gcm",
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
      suite: tenant.value.suite || "suite/aes-256-gcm",
      data_alg: "",
      mac_alg: "",
    });
  }
}

function fillRandomBase64Key(byteLen: number): void {
  const bytes = new Uint8Array(byteLen);
  window.crypto.getRandomValues(bytes);
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  patch({ tenant_key_base64: window.btoa(binary) });
  toast.info("已生成随机 Base64 密钥");
}
</script>
