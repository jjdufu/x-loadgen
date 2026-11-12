<template>
  <div class="relative flex min-h-screen w-screen items-center justify-center bg-background p-6 text-foreground">
    <!-- Top-Right Icon-Only Theme Toggle -->
    <div class="absolute right-6 top-6">
      <UiButton
        variant="ghost"
        size="icon-sm"
        :title="effectiveTheme === 'black' ? '切换为浅色模式' : '切换为纯黑模式'"
        aria-label="切换外观模式"
        @click="toggleTheme"
      >
        <Sun v-if="effectiveTheme === 'light'" class="h-4 w-4" />
        <Moon v-else class="h-4 w-4" />
      </UiButton>
    </div>

    <UiCard class="w-full max-w-md p-7 space-y-6">
      <div class="space-y-2">
        <div class="flex items-center gap-3">
          <span
            class="inline-flex h-9 w-9 items-center justify-center rounded-lg bg-foreground font-mono text-xs font-bold tracking-tighter text-background"
          >
            XL
          </span>
          <div>
            <h1 class="text-xl font-semibold tracking-tight text-foreground">
              登录
            </h1>
            <p class="text-xs text-muted-foreground">
              接口调试与压测工作台
            </p>
          </div>
        </div>

        <p class="pt-1 text-xs text-muted-foreground leading-relaxed">
          请输入管理员口令（初始口令位于数据目录
          <code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">admin.passwd</code>，首次登录后将引导修改）。
        </p>
      </div>

      <form class="space-y-4" @submit.prevent="submit">
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            管理员口令
          </label>
          <div class="flex items-center gap-1.5">
            <UiInput
              v-model="token"
              :type="show ? 'text' : 'password'"
              mono
              autocomplete="current-password"
              placeholder="输入管理员口令..."
              class="flex-1"
            />
            <UiButton
              variant="outline"
              size="icon"
              aria-label="切换口令显示"
              @click="show = !show"
            >
              <EyeOff v-if="show" class="h-4 w-4" />
              <Eye v-else class="h-4 w-4" />
            </UiButton>
          </div>
        </div>

        <UiButton
          variant="default"
          type="submit"
          :loading="loading"
          class="w-full"
        >
          登录
        </UiButton>
      </form>
    </UiCard>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { Eye, EyeOff, Moon, Sun } from "lucide-vue-next";
import { api } from "../api/workspace";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import { effectiveTheme, toggleTheme } from "../modules/theme";
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
