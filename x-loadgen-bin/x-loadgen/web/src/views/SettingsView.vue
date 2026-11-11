<template>
  <div>
    <div class="page-header">
      <h2>设置</h2>
    </div>
    <el-card v-if="authRequired" style="max-width: 480px">
      <h3>修改口令</h3>
      <el-form label-width="100px" @submit.prevent="change">
        <el-form-item label="当前口令">
          <el-input v-model="oldToken" type="password" show-password />
        </el-form-item>
        <el-form-item label="新口令">
          <el-input v-model="newToken" type="password" show-password />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" :loading="saving" native-type="submit">保存</el-button>
        </el-form-item>
      </el-form>
    </el-card>
    <el-alert v-else title="当前以 --no-auth 运行，无需改密。" type="info" show-icon :closable="false" />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ElMessage } from "element-plus";
import { http } from "../api/http";
import type { MeResponse } from "../api/types";

const authRequired = ref(false);
const oldToken = ref("");
const newToken = ref("");
const saving = ref(false);

onMounted(async () => {
  const { data } = await http.get<MeResponse>("/api/v1/auth/me");
  authRequired.value = data.auth_required;
});

async function change(): Promise<void> {
  if (newToken.value.length < 16) {
    ElMessage.error("新口令至少 16 位");
    return;
  }
  saving.value = true;
  try {
    await http.post("/api/v1/auth/password", {
      old_token: oldToken.value,
      new_token: newToken.value,
    });
    ElMessage.success("口令已更新");
    oldToken.value = "";
    newToken.value = "";
  } finally {
    saving.value = false;
  }
}
</script>
