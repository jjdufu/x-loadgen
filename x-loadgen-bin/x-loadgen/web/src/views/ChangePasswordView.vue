<template>
  <div class="min-h-screen w-screen flex items-center justify-center bg-primary p-4">
    <div class="w-full max-w-lg rounded-lg border border-dividerLight bg-primaryLight/40 p-6 shadow-xl space-y-5">
      <div class="flex items-center space-x-3">
        <KeyRound class="w-7 h-7 text-accent flex-shrink-0" />
        <div>
          <h1 class="heading">首次安全加固：修改管理员口令</h1>
          <p class="text-tiny text-secondaryLight">
            完成口令轮换后系统将自动销毁引导文件 <code class="font-mono">admin.passwd</code>
          </p>
        </div>
      </div>

      <p class="text-tiny text-secondary leading-relaxed">
        新口令不少于 <strong>24 个字符</strong>，仅限可打印 ASCII（不含空格、引号、分号、逗号和反斜杠），且不能与当前口令相同。
      </p>

      <form class="space-y-4" @submit.prevent="submit">
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            当前口令 (Current Token)
          </label>
          <input
            v-model="oldToken"
            type="password"
            autocomplete="current-password"
            placeholder="输入刚登录使用的初始口令"
            class="input font-mono"
          />
        </div>

        <div>
          <div class="flex items-center justify-between mb-1">
            <label class="text-tiny text-secondaryLight font-semibold">
              新口令 (至少 24 位可打印 ASCII)
            </label>
            <HoppButtonSecondary
              :icon="Dices"
              type="button"
              label="随机生成高强度口令"
              @click="generateStrongPassword"
            />
          </div>
          <div class="flex items-center space-x-1.5">
            <input
              v-model="newToken"
              :type="showNew ? 'text' : 'password'"
              autocomplete="new-password"
              placeholder="输入至少 24 字符的新口令"
              class="input font-mono flex-1"
            />
            <HoppButtonSecondary
              :icon="showNew ? EyeOff : Eye"
              type="button"
              outline
              @click="showNew = !showNew"
            />
          </div>
        </div>

        <HoppButtonPrimary
          :loading="loading"
          type="button"
          label="确认修改并进入工作台"
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
import { Dices, Eye, EyeOff, KeyRound } from "lucide-vue-next";
import { api } from "../api/workspace";
import { toast } from "../modules/toast";

const router = useRouter();
const oldToken = ref("");
const newToken = ref("");
const showNew = ref(false);
const loading = ref(false);

function generateStrongPassword(): void {
  const alphabet =
    "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_.~!*@#%^";
  const bytes = new Uint8Array(32);
  window.crypto.getRandomValues(bytes);
  let out = "";
  for (const b of bytes) {
    out += alphabet[b % alphabet.length];
  }
  newToken.value = out;
  showNew.value = true;
  toast.info("已生成 32 位高强度随机口令，请妥善保存");
}

async function submit(): Promise<void> {
  if (newToken.value.length < 24) {
    toast.error("新口令至少需要 24 个字符");
    return;
  }
  loading.value = true;
  try {
    await api.changePassword(oldToken.value, newToken.value);
    toast.success("管理员口令已更新");
    await router.push({ name: "workspace" });
  } finally {
    loading.value = false;
  }
}
</script>
