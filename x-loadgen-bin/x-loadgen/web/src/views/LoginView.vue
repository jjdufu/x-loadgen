<template>
  <div class="min-h-screen w-screen flex items-center justify-center bg-primary p-4">
    <div class="w-full max-w-md rounded-lg border border-dividerLight bg-primaryLight/40 p-6 shadow-xl space-y-5">
      <div class="flex items-center space-x-3">
        <span
          class="inline-flex items-center justify-center w-9 h-9 rounded bg-accent text-accentContrast font-bold text-sm shadow"
        >
          XL
        </span>
        <div>
          <h1 class="heading">登录 x-loadgen</h1>
          <p class="text-tiny text-secondaryLight">
            Hoppscotch 风格接口调试与高并发压测平台
          </p>
        </div>
      </div>

      <p class="text-tiny text-secondary leading-relaxed">
        请输入管理员访问口令（初始引导口令位于数据目录 <code class="font-mono text-accent">admin.passwd</code>，首次登录后需立即修改口令）。
      </p>

      <form class="space-y-4" @submit.prevent="submit">
        <div>
          <label class="block text-tiny text-secondaryLight mb-1.5 font-semibold">
            管理员口令 (Admin Token)
          </label>
          <div class="flex items-center space-x-1.5">
            <input
              v-model="token"
              :type="show ? 'text' : 'password'"
              autocomplete="current-password"
              placeholder="输入管理员口令..."
              class="input font-mono flex-1"
            />
            <HoppButtonSecondary
              :icon="show ? EyeOff : Eye"
              type="button"
              outline
              @click="show = !show"
            />
          </div>
        </div>

        <HoppButtonPrimary
          :loading="loading"
          type="button"
          label="登录进入工作台"
          class="w-full justify-center"
          @click="submit"
        />
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { HoppButtonPrimary, HoppButtonSecondary } from "@hoppscotch/ui";
import { Eye, EyeOff } from "lucide-vue-next";
import { api } from "../api/workspace";
import { toast } from "../modules/toast";

const router = useRouter();
const token = ref("");
const show = ref(false);
const loading = ref(false);

async function submit(): Promise<void> {
  if (!token.value.trim()) {
    toast.error("请输入管理员口令");
    return;
  }
  loading.value = true;
  try {
    const res = await api.login(token.value.trim());
    if (res.must_change_password) {
      await router.push({ name: "password" });
    } else {
      await router.push({ name: "workspace" });
    }
  } finally {
    loading.value = false;
  }
}
</script>
