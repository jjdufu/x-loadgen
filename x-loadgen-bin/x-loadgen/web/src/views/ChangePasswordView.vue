<template>
  <div class="relative flex min-h-dvh w-full items-center justify-center bg-background p-6 text-foreground">
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

    <UiCard class="w-full max-w-lg p-7 space-y-6">
      <div class="space-y-2">
        <div class="flex items-center gap-3">
          <div
            class="flex h-9 w-9 items-center justify-center rounded-lg border border-border bg-muted text-foreground shrink-0"
          >
            <KeyRound class="h-4 w-4" />
          </div>
          <div>
            <h1 class="text-xl font-semibold tracking-tight text-foreground">
              修改管理员口令
            </h1>
            <p class="text-xs text-muted-foreground">
              完成后将自动清理初始引导文件 <code class="font-mono text-foreground">admin.passwd</code>
            </p>
          </div>
        </div>

        <p class="pt-1 text-xs text-muted-foreground leading-relaxed">
          新口令不少于 <strong class="text-foreground">24 个字符</strong>，仅限可打印 ASCII（不含空格、引号、分号、逗号和反斜杠），且不能与当前口令相同。
        </p>
      </div>

      <form class="space-y-4" autocomplete="off" @submit.prevent="submit">
        <div class="space-y-1.5">
          <div class="flex h-5 items-center justify-between gap-2">
            <label
              :for="oldInputId"
              class="text-xs font-medium text-muted-foreground whitespace-nowrap"
            >
              当前口令
            </label>
          </div>
          <div class="flex items-center gap-1.5">
            <UiInput
              :id="oldInputId"
              v-model="oldToken"
              :type="showOld ? 'text' : 'password'"
              mono
              secret
              name="xlg-current-token"
              autocomplete="off"
              placeholder="输入初始口令"
              :aria-invalid="errorField === 'old'"
              class="flex-1"
              @update:model-value="clearFieldError('old')"
            />
            <UiButton
              variant="outline"
              size="icon"
              aria-label="切换当前口令显示"
              @click="showOld = !showOld"
            >
              <EyeOff v-if="showOld" class="h-4 w-4" />
              <Eye v-else class="h-4 w-4" />
            </UiButton>
          </div>
          <p v-if="errorField === 'old'" class="text-xs text-rose-600 dark:text-rose-400">
            {{ errorMessage }}
          </p>
        </div>

        <div class="space-y-1.5">
          <div class="flex h-6 items-center justify-between gap-2">
            <label
              :for="newInputId"
              class="text-xs font-medium text-muted-foreground whitespace-nowrap"
            >
              新口令
            </label>
            <button
              type="button"
              class="inline-flex h-6 cursor-pointer items-center gap-1 rounded px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              @click="handleGenerate"
            >
              <Dices class="h-3.5 w-3.5" />
              <span>随机生成</span>
            </button>
          </div>
          <div class="flex items-center gap-1.5">
            <UiInput
              :id="newInputId"
              v-model="newToken"
              :type="showNew ? 'text' : 'password'"
              mono
              secret
              name="xlg-new-token"
              autocomplete="off"
              placeholder="输入至少 24 位新口令"
              :aria-invalid="errorField === 'new'"
              class="flex-1"
              @update:model-value="clearFieldError('new')"
            />
            <UiButton
              variant="outline"
              size="icon"
              aria-label="切换新口令显示"
              @click="showNew = !showNew"
            >
              <EyeOff v-if="showNew" class="h-4 w-4" />
              <Eye v-else class="h-4 w-4" />
            </UiButton>
          </div>
          <p v-if="errorField === 'new'" class="text-xs text-rose-600 dark:text-rose-400">
            {{ errorMessage }}
          </p>
        </div>

        <div class="space-y-1.5">
          <label
            :for="confirmInputId"
            class="block text-xs font-medium text-muted-foreground whitespace-nowrap"
          >
            确认新口令
          </label>
          <UiInput
            :id="confirmInputId"
            v-model="confirmToken"
            :type="showNew ? 'text' : 'password'"
            mono
            secret
            name="xlg-confirm-token"
            autocomplete="off"
            placeholder="再次输入新口令以确认"
            :aria-invalid="errorField === 'confirm'"
            @update:model-value="clearFieldError('confirm')"
          />
          <p v-if="errorField === 'confirm'" class="text-xs text-rose-600 dark:text-rose-400">
            {{ errorMessage }}
          </p>
        </div>

        <UiButton
          variant="default"
          type="submit"
          :loading="loading"
          class="w-full"
        >
          确认修改
        </UiButton>
      </form>
    </UiCard>
  </div>
</template>

<script setup lang="ts">
import { ref, useId } from "vue";
import { useRouter } from "vue-router";
import { Dices, Eye, EyeOff, KeyRound, Moon, Sun } from "lucide-vue-next";
import { api } from "../api/workspace";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import {
  generateStrongToken,
  validateNewAdminToken,
} from "../helpers/password";
import { setCachedMe } from "../modules/auth";
import { effectiveTheme, toggleTheme } from "../modules/theme";
import { toast } from "../modules/toast";

const router = useRouter();
const oldInputId = useId();
const newInputId = useId();
const confirmInputId = useId();

const oldToken = ref("");
const newToken = ref("");
const confirmToken = ref("");
const showOld = ref(false);
const showNew = ref(false);
const loading = ref(false);
const errorField = ref<"old" | "new" | "confirm" | null>(null);
const errorMessage = ref("");

function clearFieldError(field: "old" | "new" | "confirm"): void {
  if (errorField.value === field) {
    errorField.value = null;
    errorMessage.value = "";
  }
}

function handleGenerate(): void {
  const generated = generateStrongToken(32);
  newToken.value = generated;
  confirmToken.value = generated;
  showNew.value = true;
  errorField.value = null;
  errorMessage.value = "";
  toast.info("已生成 32 位高强度随机口令，请妥善保存");
}

async function submit(): Promise<void> {
  const err = validateNewAdminToken(
    newToken.value,
    oldToken.value,
    confirmToken.value,
  );
  if (err) {
    errorField.value = err.field;
    errorMessage.value = err.message;
    return;
  }
  errorField.value = null;
  errorMessage.value = "";
  loading.value = true;
  try {
    await api.changePassword(oldToken.value, newToken.value);
    setCachedMe({
      auth_required: true,
      must_change_password: false,
    });
    toast.success("管理员口令已更新");
    await router.push({ name: "workspace" });
  } finally {
    loading.value = false;
  }
}
</script>
