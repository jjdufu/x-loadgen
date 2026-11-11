<template>
  <div>
    <div class="page-header">
      <h2>{{ isNew ? "新建目标" : "编辑目标" }}</h2>
      <el-button @click="router.push({ name: 'targets' })">返回</el-button>
    </div>
    <el-form label-width="140px">
      <el-form-item label="名称" required>
        <el-input v-model="form.name" />
      </el-form-item>
      <el-form-item label="网关地址" required>
        <el-input v-model="form.gateway_base_url" placeholder="容器可达的 IP、公网 URL 或域名" />
      </el-form-item>
      <el-form-item label="跳过校验证书">
        <el-switch v-model="form.insecure_skip_verify" />
      </el-form-item>
      <el-form-item label="备注">
        <el-input v-model="form.note" type="textarea" :rows="2" />
      </el-form-item>
      <el-form-item label="客户端证书">
        <el-input v-model="form.client_cert" type="textarea" :rows="3" />
      </el-form-item>
      <el-form-item label="客户端私钥">
        <SecretInput v-model="form.client_key" />
      </el-form-item>
      <el-form-item label="CA 证书">
        <el-input v-model="form.ca_cert" type="textarea" :rows="3" />
      </el-form-item>

      <h3>租户</h3>
      <div v-for="(tenant, index) in form.tenants" :key="index" class="block">
        <el-form-item label="名称">
          <el-input v-model="tenant.name" />
        </el-form-item>
        <el-form-item label="权重">
          <el-input-number v-model="tenant.weight" :min="1" />
        </el-form-item>
        <el-form-item label="加密模式">
          <el-select v-model="tenant.encryption_mode">
            <el-option label="none" value="none" />
            <el-option label="request" value="request" />
            <el-option label="full" value="full" />
          </el-select>
        </el-form-item>
        <el-form-item label="API Key">
          <SecretInput v-model="tenant.api_key" />
        </el-form-item>
        <el-form-item label="租户密钥">
          <SecretInput v-model="tenant.tenant_key_base64" />
        </el-form-item>
        <el-form-item label="绑定私钥">
          <SecretInput v-model="tenant.binding_key" />
        </el-form-item>
        <el-form-item label="tenant_id">
          <el-input v-model="tenant.tenant_id" />
        </el-form-item>
        <el-form-item label="key_version">
          <el-input v-model="tenant.key_version" />
        </el-form-item>
        <el-button text type="danger" @click="form.tenants.splice(index, 1)">移除此租户</el-button>
      </div>
      <el-button @click="addTenant">添加租户</el-button>

      <h3>产品</h3>
      <div v-for="(product, index) in form.products" :key="index" class="block">
        <el-form-item label="名称">
          <el-input v-model="product.name" />
        </el-form-item>
        <el-form-item label="业务产品 ID">
          <el-input v-model="product.product_id" />
        </el-form-item>
        <el-form-item label="路径">
          <el-input v-model="product.path" />
        </el-form-item>
        <el-form-item label="方法">
          <el-select v-model="product.method">
            <el-option label="GET" value="GET" />
            <el-option label="POST" value="POST" />
            <el-option label="PUT" value="PUT" />
            <el-option label="PATCH" value="PATCH" />
            <el-option label="DELETE" value="DELETE" />
          </el-select>
        </el-form-item>
        <el-form-item label="权重">
          <el-input-number v-model="product.weight" :min="1" />
        </el-form-item>
        <el-button text type="danger" @click="form.products.splice(index, 1)">移除此产品</el-button>
      </div>
      <el-button @click="addProduct">添加产品</el-button>

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
import SecretInput from "../components/SecretInput.vue";
import { http } from "../api/http";
import type { Product, TargetDetail, Tenant } from "../api/types";

interface FormTenant {
  name: string;
  weight: number;
  encryption_mode: string;
  api_key: string;
  tenant_key_base64: string;
  binding_key: string;
  tenant_id: string;
  key_version: string;
}

interface FormProduct {
  name: string;
  product_id: string;
  path: string;
  method: string;
  weight: number;
}

interface TargetForm {
  name: string;
  gateway_base_url: string;
  insecure_skip_verify: boolean;
  note: string;
  client_cert: string;
  client_key: string;
  ca_cert: string;
  tenants: FormTenant[];
  products: FormProduct[];
}

const route = useRoute();
const router = useRouter();
const isNew = computed(() => route.name === "target-new");
const saving = ref(false);
const form = reactive<TargetForm>(emptyForm());

function emptyTenant(): FormTenant {
  return {
    name: "",
    weight: 1,
    encryption_mode: "none",
    api_key: "",
    tenant_key_base64: "",
    binding_key: "",
    tenant_id: "",
    key_version: "",
  };
}

function emptyProduct(): FormProduct {
  return { name: "", product_id: "", path: "/", method: "GET", weight: 1 };
}

function emptyForm(): TargetForm {
  return {
    name: "",
    gateway_base_url: "",
    insecure_skip_verify: false,
    note: "",
    client_cert: "",
    client_key: "",
    ca_cert: "",
    tenants: [emptyTenant()],
    products: [emptyProduct()],
  };
}

function addTenant(): void {
  form.tenants.push(emptyTenant());
}

function addProduct(): void {
  form.products.push(emptyProduct());
}

function fromDetail(detail: TargetDetail): void {
  form.name = detail.name;
  form.gateway_base_url = detail.gateway_base_url;
  form.insecure_skip_verify = detail.insecure_skip_verify;
  form.note = detail.note ?? "";
  form.client_cert = detail.client_cert ?? "";
  form.client_key = detail.client_key ?? "";
  form.ca_cert = detail.ca_cert ?? "";
  form.tenants = (detail.tenants ?? []).map((tenant: Tenant) => ({
    name: tenant.name,
    weight: tenant.weight,
    encryption_mode: tenant.encryption_mode,
    api_key: tenant.api_key ?? "",
    tenant_key_base64: tenant.tenant_key_base64 ?? "",
    binding_key: tenant.binding_key ?? "",
    tenant_id: tenant.tenant_id ?? "",
    key_version: tenant.key_version ?? "",
  }));
  form.products = (detail.products ?? []).map((product: Product) => ({
    name: product.name,
    product_id: product.product_id,
    path: product.path,
    method: product.method,
    weight: product.weight,
  }));
  if (form.tenants.length === 0) addTenant();
  if (form.products.length === 0) addProduct();
}

function payload(): Record<string, unknown> {
  return {
    name: form.name,
    gateway_base_url: form.gateway_base_url,
    insecure_skip_verify: form.insecure_skip_verify,
    note: form.note || null,
    client_cert: form.client_cert || null,
    client_key: form.client_key || null,
    ca_cert: form.ca_cert || null,
    tenants: form.tenants.map((tenant) => ({
      name: tenant.name,
      weight: tenant.weight,
      encryption_mode: tenant.encryption_mode,
      api_key: tenant.api_key,
      tenant_key_base64: tenant.tenant_key_base64 || null,
      binding_key: tenant.binding_key || null,
      tenant_id: tenant.tenant_id || null,
      key_version: tenant.key_version || null,
    })),
    products: form.products.map((product) => ({
      name: product.name,
      product_id: product.product_id,
      path: product.path,
      method: product.method,
      weight: product.weight,
    })),
  };
}

async function load(): Promise<void> {
  if (isNew.value) return;
  const { data } = await http.get<TargetDetail>(`/api/v1/targets/${String(route.params.id)}`);
  fromDetail(data);
}

async function save(): Promise<void> {
  saving.value = true;
  try {
    if (isNew.value) {
      const { data } = await http.post<TargetDetail>("/api/v1/targets", payload());
      ElMessage.success("已创建");
      await router.push({ name: "target-detail", params: { id: data.id } });
    } else {
      await http.put(`/api/v1/targets/${String(route.params.id)}`, payload());
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
</style>
