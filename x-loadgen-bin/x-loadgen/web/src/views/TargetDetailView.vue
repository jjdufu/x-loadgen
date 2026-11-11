<template>
  <div>
    <div class="page-header">
      <h2>{{ isNew ? "新建目标" : "编辑目标" }}</h2>
      <el-button @click="router.push({ name: 'targets' })">返回</el-button>
    </div>
    <el-form label-width="150px">
      <el-form-item label="名称" required>
        <el-input v-model="form.name" />
      </el-form-item>
      <el-form-item label="协议" required>
        <el-radio-group v-model="form.protocol">
          <el-radio-button value="mazbri">mazbri 网关</el-radio-button>
          <el-radio-button value="http">普通 HTTP</el-radio-button>
        </el-radio-group>
        <div class="hint">
          <template v-if="isMazbri">
            先用租户 API Key 换取 token（POST /auth/token），再以 Bearer 调用 POST /api/v1/gateway；
            按租户配置做请求绑定签名和报文加密，统计与校验都基于解密后的响应。
          </template>
          <template v-else>直接请求「地址 + 产品路径」，租户 API Key（可选）以 X-API-KEY 头发送。</template>
        </div>
      </el-form-item>
      <el-form-item :label="isMazbri ? '网关地址' : '目标地址'" required>
        <el-input v-model="form.gateway_base_url" :placeholder="PLACEHOLDERS.baseUrl" />
      </el-form-item>
      <el-form-item label="跳过校验证书">
        <el-switch v-model="form.insecure_skip_verify" />
      </el-form-item>
      <el-form-item label="客户端证书">
        <el-input v-model="form.client_cert" type="textarea" :rows="3" placeholder="可选，PEM" class="mono" />
      </el-form-item>
      <el-form-item label="客户端私钥">
        <SecretInput v-model="form.client_key" multiline placeholder="可选，PEM" />
      </el-form-item>
      <el-form-item label="CA 证书">
        <el-input v-model="form.ca_cert" type="textarea" :rows="3" placeholder="可选，PEM" class="mono" />
      </el-form-item>
      <el-form-item label="备注">
        <el-input v-model="form.note" type="textarea" :rows="2" />
      </el-form-item>

      <h3>租户</h3>
      <div v-for="(tenant, index) in form.tenants" :key="index" class="block">
        <el-form-item label="名称" required>
          <el-input v-model="tenant.name" />
        </el-form-item>
        <el-form-item label="权重">
          <el-input-number v-model="tenant.weight" :min="1" />
        </el-form-item>
        <el-form-item label="API Key" :required="isMazbri">
          <SecretInput v-model="tenant.api_key" :placeholder="isMazbri ? '用于换取 token' : '可选，作为 X-API-KEY 发送'" />
        </el-form-item>

        <template v-if="isMazbri">
          <el-divider content-position="left">报文加密</el-divider>
          <el-form-item label="加密模式">
            <el-radio-group v-model="tenant.encryption_mode">
              <el-radio-button value="none">不加密</el-radio-button>
              <el-radio-button value="request">仅请求加密</el-radio-button>
              <el-radio-button value="full">请求与响应都加密</el-radio-button>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="租户 ID">
            <el-input v-model="tenant.tenant_id" placeholder="mgmt 中的租户 UUID（亦作 AEAD AAD）" />
          </el-form-item>
          <el-form-item label="密钥版本">
            <el-input v-model="tenant.key_version" placeholder="如 1" />
          </el-form-item>
          <template v-if="tenant.encryption_mode !== 'none'">
            <el-form-item label="租户密钥" required>
              <SecretInput v-model="tenant.tenant_key_base64" placeholder="32 字节密钥的标准 Base64" />
            </el-form-item>
            <el-form-item label="算法">
              <el-radio-group v-model="tenant.alg_source">
                <el-radio value="suite">命名套件</el-radio>
                <el-radio value="custom">自定义算法</el-radio>
              </el-radio-group>
            </el-form-item>
            <el-form-item v-if="tenant.alg_source === 'suite'" label="套件">
              <el-select v-model="tenant.suite" class="wide">
                <el-option v-for="item in SUITES" :key="item.value" :label="item.label" :value="item.value" />
              </el-select>
            </el-form-item>
            <template v-else>
              <el-form-item label="数据算法">
                <el-select v-model="tenant.data_alg" class="wide">
                  <el-option v-for="item in DATA_ALGS" :key="item.value" :label="item.label" :value="item.value" />
                </el-select>
              </el-form-item>
              <el-form-item label="MAC 算法">
                <el-select v-model="tenant.mac_alg" class="wide">
                  <el-option v-for="item in MAC_ALGS" :key="item.value" :label="item.label" :value="item.value" />
                </el-select>
              </el-form-item>
            </template>
            <el-form-item label="信封签名">
              <el-select v-model="tenant.require_signature">
                <el-option label="默认（有 MAC 时要求）" value="" />
                <el-option label="要求" value="true" />
                <el-option label="不要求" value="false" />
              </el-select>
            </el-form-item>
            <el-form-item label="允许不安全算法">
              <el-switch v-model="tenant.allow_insecure" />
              <span class="hint inline">仅为兼容 ECB / DES / HMAC-MD5 等 legacy 算法时打开</span>
            </el-form-item>
            <el-alert
              v-if="usesLegacy(tenant) && !tenant.allow_insecure"
              type="warning"
              :closable="false"
              show-icon
              title="所选算法属于 legacy，需打开「允许不安全算法」才能保存"
            />
          </template>

          <el-divider content-position="left">请求绑定（x-mazbri-* 签名头）</el-divider>
          <el-form-item label="绑定算法">
            <el-select v-model="tenant.binding_alg" class="wide">
              <el-option v-for="item in BINDING_ALGS" :key="item.value" :label="item.label" :value="item.value" />
            </el-select>
          </el-form-item>
          <template v-if="tenant.binding_alg">
            <el-form-item label="Key ID" required>
              <el-input v-model="tenant.binding_kid" placeholder="mgmt 中登记的 kid" />
            </el-form-item>
            <el-form-item :label="isHmacBinding(tenant.binding_alg) ? 'HMAC 密钥' : '签名私钥'" required>
              <SecretInput
                v-if="isHmacBinding(tenant.binding_alg)"
                v-model="tenant.binding_key"
                placeholder="mgmt 签发的标准 Base64 密钥"
              />
              <SecretInput v-else v-model="tenant.binding_key" multiline placeholder="PKCS#8 PEM 私钥，或点击下方按钮生成" />
            </el-form-item>
            <el-form-item v-if="!isHmacBinding(tenant.binding_alg)" label="密钥对">
              <el-button
                :disabled="!canGenerate(tenant)"
                :loading="generating === index"
                @click="generateKeypair(tenant, index)"
              >
                生成签名密钥对
              </el-button>
              <span class="hint inline">
                {{ canGenerate(tenant) ? "私钥保存在本机数据库，公钥用于在 mgmt 登记" : "先保存目标，再为该租户生成" }}
              </span>
            </el-form-item>
            <el-form-item v-if="tenant.binding_public_key" label="公钥">
              <CopyableText :value="tenant.binding_public_key" />
            </el-form-item>
          </template>
        </template>
        <el-button text type="danger" @click="form.tenants.splice(index, 1)">移除此租户</el-button>
      </div>
      <el-button @click="form.tenants.push(emptyTenant())">添加租户</el-button>

      <h3>产品</h3>
      <div v-for="(product, index) in form.products" :key="index" class="block">
        <el-form-item label="名称" required>
          <el-input v-model="product.name" />
        </el-form-item>
        <el-form-item label="产品 ID" required>
          <el-input v-model="product.product_id" :placeholder="isMazbri ? 'GatewayRequest.product_id' : '用于统计分组'" />
        </el-form-item>
        <el-form-item :label="isMazbri ? '上游路径' : '路径'" required>
          <el-input v-model="product.path" :placeholder="isMazbri ? 'GatewayRequest.path' : '拼接在目标地址之后'" />
        </el-form-item>
        <el-form-item label="方法">
          <el-select v-model="product.method">
            <el-option v-for="method in METHODS" :key="method" :label="method" :value="method" />
          </el-select>
        </el-form-item>
        <el-form-item label="权重">
          <el-input-number v-model="product.weight" :min="1" />
        </el-form-item>
        <el-form-item label="超时（秒）">
          <el-input-number v-model="product.timeout_secs" :min="1" :value-on-clear="null" placeholder="沿用模板" />
        </el-form-item>
        <el-form-item label="Query（JSON）">
          <el-input v-model="product.query_text" type="textarea" :rows="2" :placeholder="PLACEHOLDERS.query" class="mono" />
        </el-form-item>
        <el-form-item label="Headers（JSON）">
          <el-input v-model="product.headers_text" type="textarea" :rows="2" :placeholder="PLACEHOLDERS.headers" class="mono" />
        </el-form-item>
        <el-form-item label="Body（JSON）">
          <el-input v-model="product.body_text" type="textarea" :rows="4" :placeholder="PLACEHOLDERS.body" class="mono" />
        </el-form-item>
        <el-form-item label="期望">
          <el-radio-group v-model="product.expect_transport">
            <el-radio value="ok">调用成功</el-radio>
            <el-radio value="reject">被拒绝</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="业务校验（JSON）">
          <el-input
            v-model="product.checks_text"
            type="textarea"
            :rows="2"
            :placeholder="PLACEHOLDERS.checks"
            class="mono"
          />
          <div class="hint">{{ isMazbri ? "在解密后的 data 上执行" : "在响应 JSON 上执行" }}；不写 eq 表示字段必须存在且非 null。</div>
        </el-form-item>
        <el-button text type="danger" @click="form.products.splice(index, 1)">移除此产品</el-button>
      </div>
      <el-button @click="form.products.push(emptyProduct())">添加产品</el-button>

      <el-form-item class="actions">
        <el-button type="primary" :loading="saving" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import CopyableText from "../components/CopyableText.vue";
import SecretInput from "../components/SecretInput.vue";
import { http } from "../api/http";
import type { KeypairResponse, TargetDetail, Tenant } from "../api/types";
import {
  BINDING_ALGS,
  DATA_ALGS,
  LEGACY_ALGS,
  MAC_ALGS,
  SUITES,
  emptyForm,
  emptyProduct,
  emptyTenant,
  formFromDetail,
  isHmacBinding,
  targetPayload,
  tenantFromApi,
  type FormTenant,
  type TargetForm,
} from "./targetForm";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"];
const PLACEHOLDERS = {
  baseUrl: "容器可达的 IP、公网 URL 或域名，如 https://gw.example.com",
  query: '{"page": "{{seq}}"}',
  headers: '{"x-trace": "t-{{seq}}"}',
  body: '{"order_no": "o-{{seq}}"}',
  checks: '[{"path": "code", "eq": 0}, {"path": "id"}]',
};

const route = useRoute();
const router = useRouter();
const form = reactive<TargetForm>(emptyForm());
const isNew = computed(() => route.name === "target-new");
const isMazbri = computed(() => form.protocol === "mazbri");
const saving = ref(false);
const generating = ref<number | null>(null);

function targetId(): string {
  return String(route.params.id);
}

function usesLegacy(tenant: FormTenant): boolean {
  return tenant.alg_source === "custom" && (LEGACY_ALGS.has(tenant.data_alg) || LEGACY_ALGS.has(tenant.mac_alg));
}

function canGenerate(tenant: FormTenant): boolean {
  return !isNew.value && tenant.id !== "" && tenant.binding_kid.trim() !== "";
}

function applyDetail(detail: TargetDetail): void {
  Object.assign(form, formFromDetail(detail));
}

async function load(): Promise<void> {
  if (isNew.value) return;
  const { data } = await http.get<TargetDetail>(`/api/v1/targets/${targetId()}`);
  applyDetail(data);
}

async function generateKeypair(tenant: FormTenant, index: number): Promise<void> {
  generating.value = index;
  try {
    await http.post<KeypairResponse>(
      `/api/v1/targets/${targetId()}/tenants/${tenant.id}/binding-keypair`,
      { alg: tenant.binding_alg, kid: tenant.binding_kid.trim() },
    );
    const { data } = await http.get<Tenant>(`/api/v1/targets/${targetId()}/tenants/${tenant.id}`);
    const fresh = tenantFromApi(data);
    tenant.binding_alg = fresh.binding_alg;
    tenant.binding_kid = fresh.binding_kid;
    tenant.binding_key = fresh.binding_key;
    tenant.binding_public_key = fresh.binding_public_key;
    ElMessage.success("已生成密钥对：私钥已保存，请复制公钥到 mgmt 登记");
  } finally {
    generating.value = null;
  }
}

async function save(): Promise<void> {
  let body: Record<string, unknown>;
  try {
    body = targetPayload(form);
  } catch (error) {
    ElMessage.error(error instanceof Error ? error.message : String(error));
    return;
  }
  saving.value = true;
  try {
    if (isNew.value) {
      const { data } = await http.post<TargetDetail>("/api/v1/targets", body);
      ElMessage.success("已创建");
      await router.push({ name: "target-detail", params: { id: data.id } });
      applyDetail(data);
    } else {
      const { data } = await http.put<TargetDetail>(`/api/v1/targets/${targetId()}`, body);
      applyDetail(data);
      ElMessage.success("已保存");
    }
  } finally {
    saving.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<style scoped>
.block {
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
  background: #fff;
}
h3 {
  margin: 24px 0 8px;
}
.actions {
  margin-top: 24px;
}
.hint {
  color: #6b7280;
  font-size: 12px;
  line-height: 1.6;
  width: 100%;
}
.hint.inline {
  width: auto;
  margin-left: 12px;
}
.wide {
  width: 420px;
}
.mono :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
