<template>
  <div class="login">
    <el-card class="box">
      <h2>修改口令</h2>
      <p class="hint">新口令不少于 16 位，且不能与当前口令相同。完成前无法进入业务页。</p>
      <el-form @submit.prevent="submit">
        <el-form-item label="当前口令">
          <el-input v-model="oldToken" type="password" show-password />
        </el-form-item>
        <el-form-item label="新口令">
          <el-input v-model="newToken" type="password" show-password />
        </el-form-item>
        <el-button type="primary" :loading="loading" native-type="submit" style="width: 100%">
          确认修改
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
const oldToken = ref("");
const newToken = ref("");
const loading = ref(false);

async function submit(): Promise<void> {
  if (newToken.value.length < 16) {
    ElMessage.error("新口令至少 16 位");
    return;
  }
  loading.value = true;
  try {
    await http.post("/api/v1/auth/password", {
      old_token: oldToken.value,
      new_token: newToken.value,
    });
    ElMessage.success("口令已更新");
    await router.push({ name: "runs" });
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
  background: #f5f7fa;
}
.box {
  width: 420px;
}
.hint {
  color: #6b7280;
  font-size: 13px;
}
</style>
