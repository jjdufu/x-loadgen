<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-6xl space-y-6 p-6 lg:p-8">
      <!-- Balanced 2-Column Shadcn Card Grid -->
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <!-- Card 1: Light / Black Theme Switcher -->
        <UiCard class="flex flex-col justify-between p-6 space-y-5">
          <div class="space-y-1.5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              外观模式
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              默认自动跟随操作系统配色方案，也可手动固定为浅色或纯黑模式。
            </p>
          </div>

          <!-- Visual Mini-Window Previews: Light & Black Only -->
          <div class="grid grid-cols-2 gap-4">
            <!-- Light Preview -->
            <button
              type="button"
              class="group cursor-pointer rounded-xl border-2 p-2.5 text-left transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :class="
                effectiveTheme === 'light'
                  ? 'border-foreground bg-accent/35'
                  : 'border-border hover:border-muted-foreground/50'
              "
              @click="setThemeMode('light')"
            >
              <div class="overflow-hidden rounded-lg border border-zinc-200 bg-white p-3 space-y-2 shadow-sm">
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
                  <span>浅色</span>
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
              class="group cursor-pointer rounded-xl border-2 p-2.5 text-left transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :class="
                effectiveTheme === 'black'
                  ? 'border-foreground bg-accent/35'
                  : 'border-border hover:border-muted-foreground/50'
              "
              @click="setThemeMode('black')"
            >
              <div class="overflow-hidden rounded-lg border border-zinc-800 bg-[#09090b] p-3 space-y-2 shadow-sm">
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
                  <span>纯黑</span>
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
                    ? `跟随系统（${effectiveTheme === "black" ? "纯黑" : "浅色"}）`
                    : `固定为${effectiveTheme === "black" ? "纯黑" : "浅色"}`
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
              <span>跟随系统</span>
            </UiButton>
          </div>
        </UiCard>

        <!-- Card 2: Instance Capacity -->
        <UiCard class="flex flex-col justify-between p-6 space-y-5">
          <div class="space-y-1.5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              实例规格
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              当前守护进程的版本、鉴权状态及全局发压安全护栏配置。
            </p>
          </div>

          <div v-if="serverMeta" class="grid grid-cols-2 gap-3.5">
            <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-1">
              <div class="text-xs text-muted-foreground">服务端版本</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                v{{ serverMeta.version }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-1">
              <div class="text-xs text-muted-foreground">鉴权状态</div>
              <div class="text-sm font-semibold text-foreground">
                {{ serverMeta.auth_required ? "口令保护已启用" : "免密模式 (--no-auth)" }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-1">
              <div class="text-xs text-muted-foreground">全局 QPS 上限</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                {{ serverMeta.max_qps ?? "未限制" }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-1">
              <div class="text-xs text-muted-foreground">全局并发上限</div>
              <div class="font-mono text-base font-semibold text-foreground tabular-nums">
                {{ serverMeta.max_concurrency ?? "未限制" }}
              </div>
            </div>
          </div>
        </UiCard>
      </div>

      <!-- Card 3: Keyboard Shortcut Management -->
      <UiCard class="p-6 space-y-4">
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="space-y-1">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              快捷键管理
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              点击键位按钮即可录制新组合键（需包含 Ctrl / ⌘ 或 Alt）；录制时按 Esc 取消，按 Backspace 清空禁用。
            </p>
          </div>
          <UiButton variant="outline" size="sm" @click="handleResetShortcuts">
            <RotateCcw class="h-3.5 w-3.5" />
            <span>恢复默认</span>
          </UiButton>
        </div>

        <div class="divide-y divide-border overflow-hidden rounded-lg border border-border">
          <div
            v-for="item in SHORTCUT_DEFINITIONS"
            :key="item.id"
            class="flex flex-wrap items-center justify-between gap-3 bg-background px-4 py-3"
          >
            <div class="min-w-0 space-y-0.5">
              <div class="text-sm font-medium text-foreground">
                {{ item.label }}
              </div>
              <div class="text-xs text-muted-foreground">
                {{ item.description }}
              </div>
            </div>

            <div class="flex items-center gap-1.5">
              <button
                type="button"
                class="inline-flex h-8 min-w-[128px] cursor-pointer items-center justify-center rounded-md border px-3 font-mono text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                :class="
                  recordingShortcutId === item.id
                    ? 'border-foreground bg-accent text-foreground font-semibold'
                    : shortcutBindings[item.id]
                      ? 'border-border bg-surface text-foreground hover:bg-accent/60'
                      : 'border-dashed border-border text-muted-foreground hover:text-foreground'
                "
                @click="toggleRecord(item.id)"
              >
                {{
                  recordingShortcutId === item.id
                    ? "请按下组合键..."
                    : formatBindingDisplay(shortcutBindings[item.id])
                }}
              </button>

              <UiButton
                v-if="shortcutBindings[item.id]"
                variant="ghost"
                size="icon-sm"
                title="清空快捷键"
                aria-label="清空快捷键"
                @click="handleClearShortcut(item.id)"
              >
                <X class="h-3.5 w-3.5" />
              </UiButton>
            </div>
          </div>
        </div>
      </UiCard>

      <!-- Card 4: Security / Admin Password Rotation Card -->
      <UiCard class="p-6">
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-12">
          <div class="space-y-1.5 lg:col-span-5">
            <h2 class="text-base font-semibold tracking-tight text-foreground">
              访问口令轮换
            </h2>
            <p class="text-xs text-muted-foreground leading-relaxed">
              新口令不少于 24 位可打印 ASCII（不含空格、引号、分号、逗号和反斜杠）。更新后旧口令与历史会话立即失效。
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
              免密模式运行。生产或公网部署请移除该启动参数以启用口令保护。
            </div>

            <form v-else class="space-y-4" autocomplete="off" @submit.prevent="changePwd">
              <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                <div class="space-y-1.5">
                  <div class="flex h-5 items-center justify-between gap-2">
                    <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
                      当前口令
                    </label>
                  </div>
                  <UiInput
                    v-model="oldToken"
                    type="password"
                    mono
                    autocomplete="off"
                    placeholder="输入当前口令"
                  />
                </div>

                <div class="space-y-1.5">
                  <div class="flex h-5 items-center justify-between gap-2">
                    <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
                      新口令（至少 24 位）
                    </label>
                    <button
                      type="button"
                      class="inline-flex h-5 shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 text-xs font-medium whitespace-nowrap text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                      @click="generateStrongPassword"
                    >
                      <Dices class="h-3 w-3" />
                      <span>随机生成</span>
                    </button>
                  </div>
                  <div class="flex items-center gap-1.5">
                    <UiInput
                      v-model="newToken"
                      :type="showNew ? 'text' : 'password'"
                      mono
                      autocomplete="off"
                      placeholder="至少 24 位可打印 ASCII"
                      class="flex-1"
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
                </div>
              </div>

              <div class="flex justify-end">
                <UiButton
                  variant="default"
                  type="submit"
                  :loading="saving"
                >
                  更新口令
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
import { onBeforeUnmount, onMounted, ref } from "vue";
import {
  Check,
  Dices,
  Eye,
  EyeOff,
  Moon,
  RotateCcw,
  Sun,
  X,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import { canProgrammaticallyFocus } from "../modules/activity";
import {
  SHORTCUT_DEFINITIONS,
  captureBindingFromEvent,
  clearShortcutBinding,
  formatBindingDisplay,
  recordingShortcutId,
  resetAllShortcuts,
  setShortcutBinding,
  shortcutBindings,
  type ShortcutId,
} from "../modules/shortcuts";
import {
  effectiveTheme,
  isFollowingSystem,
  resetToSystemTheme,
  setThemeMode,
} from "../modules/theme";
import { toast } from "../modules/toast";
import { refreshMeta, serverMeta } from "../stores/workspace";

const authRequired = ref(false);
const oldToken = ref("");
const newToken = ref("");
const showNew = ref(false);
const saving = ref(false);

function toggleRecord(id: ShortcutId): void {
  recordingShortcutId.value = recordingShortcutId.value === id ? null : id;
}

function handleClearShortcut(id: ShortcutId): void {
  if (recordingShortcutId.value === id) {
    recordingShortcutId.value = null;
  }
  clearShortcutBinding(id);
  toast.info("已清空该快捷键");
}

function handleResetShortcuts(): void {
  recordingShortcutId.value = null;
  resetAllShortcuts();
  toast.success("已恢复默认快捷键");
}

function handleRecorderKeydown(e: KeyboardEvent): void {
  if (!canProgrammaticallyFocus()) return;
  const targetId = recordingShortcutId.value;
  if (!targetId) return;

  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    recordingShortcutId.value = null;
    return;
  }

  if (
    (e.key === "Backspace" || e.key === "Delete") &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey
  ) {
    clearShortcutBinding(targetId);
    recordingShortcutId.value = null;
    toast.info("已清空该快捷键");
    return;
  }

  const captured = captureBindingFromEvent(e);
  if (!captured.binding) {
    if (captured.reason === "needs_modifier") {
      toast.error("请组合 Ctrl / ⌘ 或 Alt 键设置快捷键");
    }
    return;
  }

  const res = setShortcutBinding(targetId, captured.binding);
  if (!res.ok) {
    toast.error(`快捷键与「${res.conflictLabel}」冲突`);
    return;
  }

  recordingShortcutId.value = null;
  toast.success(`已更新为 ${formatBindingDisplay(captured.binding)}`);
}

onMounted(async () => {
  window.addEventListener("keydown", handleRecorderKeydown, true);
  await refreshMeta();
  const me = await api.me();
  authRequired.value = me.auth_required;
});

onBeforeUnmount(() => {
  recordingShortcutId.value = null;
  window.removeEventListener("keydown", handleRecorderKeydown, true);
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
    toast.success("口令已更新");
    oldToken.value = "";
    newToken.value = "";
  } finally {
    saving.value = false;
  }
}
</script>
