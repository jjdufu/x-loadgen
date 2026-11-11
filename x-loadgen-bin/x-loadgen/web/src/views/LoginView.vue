<template>
  <div class="login">
    <el-card class="box">
      <h2>登录 x-loadgen</h2>
      <p class="hint">请输入管理员口令。首次登录后需要立即改密。</p>
      <el-form @submit.prevent="submit">
        <el-form-item label="口令">
          <el-input v-model="token" type="password" show-password autocomplete="current-password" />
        </el-form-item>
        <el-button type="primary" :loading="loading" native-type="submit" style="width: 100%">
          登录
        </el-button>
      </el-form>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import { http } from "../api/http";

const router = useRouter();
const token = ref("");
const loading = ref(false);

async function submit(): Promise<void> {
  if (!token.value) {
    ElMessage.error("请输入口令");
    return;
  }
  loading.value = true;
  try {
    const { data } = await http.post<{ must_change_password: boolean }>("/api/v1/auth/login", {
      token: token.value,
    });
    if (data.must_change_password) {
      await router.push({ name: "password" });
    } else {
      await router.push({ name: "runs" });
    }
  } finally {
    loading.value = false;
  }
}
</script>

<style scoped>
.login {
  min-height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}
.box {
  width: 380px;
}
.hint {
  color: #6b7280;
  font-size: 13px;
}
</style>
