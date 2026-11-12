<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-6xl space-y-8 p-6 lg:p-8">
      <!-- Page Title -->
      <div class="space-y-1.5 border-b border-border pb-5">
        <h1 class="text-2xl font-semibold tracking-tight text-foreground">
          系统外观与安全设置
        </h1>
        <p class="text-sm text-muted-foreground">
          切换浅色（Light）与纯黑（Black）显示模式，查看当前实例的压测容量护栏及管理访问口令。
        </p>
      </div>

      <!-- Balanced 2-Column Shadcn Card Grid -->
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <!-- Card 1: Light / Black Theme Switcher -->
        <UiCard class="flex flex-col justify-between p-6 space-y-5">
          <div class="space-y-1.5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              界面明暗模式 (Appearance)
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              首次访问默认自动跟随操作系统配色方案；您也可以在此或右上角一键切换为浅色（Light）或纯黑（Black）。
            </p>
          </div>

          <!-- Visual Mini-Window Previews: Light & Black Only -->
          <div class="grid grid-cols-2 gap-4">
            <!-- Light Preview -->
            <button
              type="button"
              class="group rounded-xl border-2 p-2.5 text-left transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :class="
                effectiveTheme === 'light'
                  ? 'border-foreground bg-accent/35'
                  : 'border-border hover:border-muted-foreground/50'
              "
              @click="setThemeMode('light')"
            >
              <div class="overflow-hidden rounded-lg border border-zinc-200 bg-white p-3 space-y-2 shadow-2xs">
                <div class="flex items-center justify-between">
                  <div class="h-2 w-12 rounded bg-zinc-900" />
                  <div class="h-3.5 w-9 rounded bg-zinc-900" />
                </div>
                <div class="h-2 w-4/5 rounded bg-zinc-200" />
                <div class="grid grid-cols-2 gap-1.5 pt-1">
                  <div class="h-6 rounded border border-zinc-200 bg-zinc-50" />
                  <div class="h-6 rounded border border-zinc-200 bg-zinc-50" />
                </div>
              </div>
              <div class="mt-2.5 flex items-center justify-between px-0.5">
                <span class="inline-flex items-center gap-1.5 text-sm font-medium text-foreground">
                  <Sun class="h-3.5 w-3.5" />
                  <span>浅色 (Light)</span>
                </span>
                <Check
                  v-if="effectiveTheme === 'light'"
                  class="h-4 w-4 text-foreground"
                />
              </div>
            </button>

            <!-- Black Preview -->
            <button
              type="button"
              class="group rounded-xl border-2 p-2.5 text-left transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :class="
                effectiveTheme === 'black'
                  ? 'border-foreground bg-accent/35'
                  : 'border-border hover:border-muted-foreground/50'
              "
              @click="setThemeMode('black')"
            >
              <div class="overflow-hidden rounded-lg border border-zinc-800 bg-[#09090b] p-3 space-y-2 shadow-2xs">
                <div class="flex items-center justify-between">
                  <div class="h-2 w-12 rounded bg-zinc-100" />
                  <div class="h-3.5 w-9 rounded bg-zinc-100" />
                </div>
                <div class="h-2 w-4/5 rounded bg-zinc-800" />
                <div class="grid grid-cols-2 gap-1.5 pt-1">
                  <div class="h-6 rounded border border-zinc-800 bg-zinc-900" />
                  <div class="h-6 rounded border border-zinc-800 bg-zinc-900" />
                </div>
              </div>
              <div class="mt-2.5 flex items-center justify-between px-0.5">
                <span class="inline-flex items-center gap-1.5 text-sm font-medium text-foreground">
                  <Moon class="h-3.5 w-3.5" />
                  <span>纯黑 (Black)</span>
                </span>
                <Check
                  v-if="effectiveTheme === 'black'"
                  class="h-4 w-4 text-foreground"
                />
              </div>
            </button>
          </div>

          <div
            class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-surface/60 px-3.5 py-2.5 text-xs"
          >
            <span class="text-muted-foreground">
              当前策略：
              <strong class="font-medium text-foreground">
                {{
                  isFollowingSystem
                    ? `自动跟随系统 (${effectiveTheme === "black" ? "Black" : "Light"})`
                    : `手动固定为 ${effectiveTheme === "black" ? "纯黑 (Black)" : "浅色 (Light)"}`
                }}
              </strong>
            </span>
            <UiButton
              v-if="!isFollowingSystem"
              variant="ghost"
              size="xs"
              @click="resetToSystemTheme"
            >
              <RotateCcw class="h-3 w-3" />
              <span>恢复默认跟随系统</span>
            </UiButton>
          </div>
        </UiCard>

        <!-- Card 2: Instance Capacity & Built-in OpenAPI Reference -->
        <UiCard class="flex flex-col justify-between p-6 space-y-5">
          <div class="space-y-1.5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              实例运行规格与接口文档
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              当前守护进程的全局速率/并发安全护栏配置，以及离线内置的 OpenAPI 3 接口规范。
            </p>
          </div>

          <div v-if="serverMeta" class="grid grid-cols-2 gap-3.5">
            <div class="rounded-lg border border-border bg-surface/50 p-3.5 space-y-1">
              <div class="text-xs text-muted-foreground">服务端版本</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                v{{ serverMeta.version }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-3.5 space-y-1">
              <div class="text-xs text-muted-foreground">访问鉴权状态</div>
              <div class="text-sm font-semibold text-foreground">
                {{ serverMeta.auth_required ? "Admin 口令鉴权" : "--no-auth 免密模式" }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-3.5 space-y-1">
              <div class="text-xs text-muted-foreground">全局 QPS 上限</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                {{ serverMeta.max_qps ?? "未限制" }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-3.5 space-y-1">
              <div class="text-xs text-muted-foreground">全局并发上限</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                {{ serverMeta.max_concurrency ?? "未限制" }}
              </div>
            </div>
          </div>

          <div
            class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-surface/60 px-4 py-3"
          >
            <div class="space-y-0.5">
              <div class="text-xs font-semibold text-foreground">
                OpenAPI 3 接口规范文档
              </div>
              <div class="text-xs text-muted-foreground">
                浏览离线渲染的接口参数、Schema 与 cURL 示例
              </div>
            </div>
            <UiButton
              variant="outline"
              size="sm"
              @click="router.push({ name: 'docs' })"
            >
              <BookOpen class="h-3.5 w-3.5" />
              <span>打开接口文档</span>
            </UiButton>
          </div>
        </UiCard>
      </div>

      <!-- Full-Width Security / Admin Password Rotation Card -->
      <UiCard class="p-6">
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-12">
          <div class="space-y-1.5 lg:col-span-5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              管理员访问口令轮换
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              新口令不少于 24 个字符，仅限可打印 ASCII（不含空格、引号、分号、逗号和反斜杠）。修改成功后旧口令与历史 Cookie 会话立即失效。
            </p>
          </div>

          <div class="lg:col-span-7">
            <div
              v-if="!authRequired"
              class="rounded-lg border border-border bg-surface/50 p-4 text-sm text-muted-foreground"
            >
              当前实例以
              <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground">
                --no-auth
              </code>
              模式启动，无需口令即可访问。生产或公网部署请移除该启动标志以启用口令保护。
            </div>

            <form v-else class="space-y-4" @submit.prevent="changePwd">
              <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <div class="space-y-1.5">
                  <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                    当前口令
                  </label>
                  <UiInput
                    v-model="oldToken"
                    type="password"
                    mono
                    autocomplete="current-password"
                    placeholder="输入当前管理员口令"
                  />
                </div>

                <div class="space-y-1.5">
                  <div class="flex items-center justify-between gap-2">
                    <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
                      新口令 (&gt;= 24 位)
                    </label>
                    <UiButton
                      variant="ghost"
                      size="xs"
                      @click="generateStrongPassword"
                    >
                      <Dices class="h-3 w-3" />
                      <span>随机生成</span>
                    </UiButton>
                  </div>
                  <div class="flex items-center gap-1.5">
                    <UiInput
                      v-model="newToken"
                      :type="showNew ? 'text' : 'password'"
                      mono
                      autocomplete="new-password"
                      placeholder="至少 24 位可打印 ASCII"
                      class="flex-1"
                    />
                    <UiButton
                      variant="outline"
                      size="icon"
                      @click="showNew = !showNew"
                    >
                      <EyeOff v-if="showNew" class="h-4 w-4" />
                      <Eye v-else class="h-4 w-4" />
                    </UiButton>
                  </div>
                </div>
              </div>

              <div class="flex justify-end">
                <UiButton
                  variant="default"
                  :loading="saving"
                  @click="changePwd"
                >
                  更新管理员口令
                </UiButton>
              </div>
            </form>
          </div>
        </div>
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  BookOpen,
  Check,
  Dices,
  Eye,
  EyeOff,
  Moon,
  RotateCcw,
  Sun,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import {
  effectiveTheme,
  isFollowingSystem,
  resetToSystemTheme,
  setThemeMode,
} from "../modules/theme";
import { toast } from "../modules/toast";
import { refreshMeta, serverMeta } from "../stores/workspace";

const router = useRouter();
const authRequired = ref(false);
const oldToken = ref("");
const newToken = ref("");
const showNew = ref(false);
const saving = ref(false);

onMounted(async () => {
  await refreshMeta();
  const me = await api.me();
  authRequired.value = me.auth_required;
});

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

async function changePwd(): Promise<void> {
  if (newToken.value.length < 24) {
    toast.error("新口令至少需要 24 个字符");
    return;
  }
  saving.value = true;
  try {
    await api.changePassword(oldToken.value, newToken.value);
    toast.success("管理员口令已更新");
    oldToken.value = "";
    newToken.value = "";
  } finally {
    saving.value = false;
  }
}
</script>
