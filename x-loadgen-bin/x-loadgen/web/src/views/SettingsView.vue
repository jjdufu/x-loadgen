<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-y-auto p-6 space-y-8 max-w-4xl">
    <div class="border-b border-dividerLight pb-4">
      <h1 class="heading">外观主题与平台设置 (Settings)</h1>
      <p class="text-tiny text-secondaryLight mt-1">
        自定义 Hoppscotch 配色方案、强调色主题，并查看服务端压测容量上限或修改管理员口令。
      </p>
    </div>

    <!-- Hoppscotch Appearance: Background & Accent Swatches -->
    <section class="space-y-4">
      <h2 class="font-bold text-secondaryDark">界面主题配色 (Hoppscotch Theme)</h2>

      <div>
        <div class="text-tiny text-secondaryLight mb-2 font-semibold">背景模式 (Background)</div>
        <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
          <button
            v-for="mode in HOPP_BG_COLORS"
            :key="mode.value"
            type="button"
            class="p-3 rounded border text-left transition"
            :class="
              bgColor === mode.value
                ? 'border-accent bg-accent/10'
                : 'border-dividerLight hover:border-dividerDark bg-primaryLight/30'
            "
            @click="bgColor = mode.value"
          >
            <div class="font-bold text-secondaryDark text-tiny">{{ mode.label }}</div>
            <div class="text-[11px] text-secondaryLight mt-0.5">{{ mode.desc }}</div>
          </button>
        </div>
      </div>

      <div>
        <div class="text-tiny text-secondaryLight mb-2 font-semibold">强调色 (Accent Color)</div>
        <div class="flex flex-wrap items-center gap-2.5">
          <button
            v-for="acc in HOPP_ACCENT_COLORS"
            :key="acc.value"
            v-tippy="acc.label"
            type="button"
            class="flex items-center space-x-2 px-3 py-1.5 rounded border text-tiny font-semibold transition"
            :class="
              accentColor === acc.value
                ? 'border-accent bg-accent/15 text-secondaryDark'
                : 'border-dividerLight text-secondary hover:text-secondaryDark'
            "
            @click="accentColor = acc.value"
          >
            <span
              class="w-3.5 h-3.5 rounded-full"
              :style="{ backgroundColor: acc.hex }"
            />
            <span>{{ acc.label }}</span>
          </button>
        </div>
      </div>
    </section>

    <!-- Server Capacity & Runtime Info -->
    <section v-if="serverMeta" class="space-y-3 pt-4 border-t border-dividerLight">
      <h2 class="font-bold text-secondaryDark">实例运行指标与容量护栏</h2>
      <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 text-tiny font-mono">
        <div class="p-3 rounded border border-dividerLight bg-primaryLight/30">
          <div class="text-secondaryLight font-sans">服务端版本</div>
          <div class="text-secondaryDark font-bold mt-1">v{{ serverMeta.version }}</div>
        </div>
        <div class="p-3 rounded border border-dividerLight bg-primaryLight/30">
          <div class="text-secondaryLight font-sans">全局 QPS 上限</div>
          <div class="text-secondaryDark font-bold mt-1">
            {{ serverMeta.max_qps ?? "未限制" }}
          </div>
        </div>
        <div class="p-3 rounded border border-dividerLight bg-primaryLight/30">
          <div class="text-secondaryLight font-sans">全局并发上限</div>
          <div class="text-secondaryDark font-bold mt-1">
            {{ serverMeta.max_concurrency ?? "未限制" }}
          </div>
        </div>
        <div class="p-3 rounded border border-dividerLight bg-primaryLight/30">
          <div class="text-secondaryLight font-sans">认证模式</div>
          <div class="text-accent font-bold mt-1">
            {{ serverMeta.auth_required ? "Admin 口令鉴权" : "--no-auth 免密模式" }}
          </div>
        </div>
      </div>
    </section>

    <!-- Password Rotation -->
    <section class="space-y-3 pt-4 border-t border-dividerLight">
      <h2 class="font-bold text-secondaryDark">管理员访问口令</h2>
      <div
        v-if="!authRequired"
        class="p-4 rounded border border-dividerLight bg-primaryLight/30 text-tiny text-secondary"
      >
        当前实例以 <code class="font-mono text-accent">--no-auth</code> 参数启动，无需口令即可访问。
      </div>

      <form v-else class="space-y-3 max-w-md" @submit.prevent="changePwd">
        <p class="text-tiny text-secondaryLight">
          新口令不少于 24 个字符，仅限可打印 ASCII（不含空格、引号、分号、逗号和反斜杠）。改密后旧会话与旧口令立即失效。
        </p>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">当前口令</label>
          <input v-model="oldToken" type="password" class="input font-mono" />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">新口令 (&gt;= 24 位)</label>
          <input v-model="newToken" type="password" class="input font-mono" />
        </div>
        <HoppButtonPrimary :loading="saving" type="button" label="更新管理员口令" @click="changePwd" />
      </form>
    </section>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { HoppButtonPrimary } from "@hoppscotch/ui";
import { api } from "../api/workspace";
import {
  HOPP_ACCENT_COLORS,
  HOPP_BG_COLORS,
  accentColor,
  bgColor,
} from "../modules/theming";
import { toast } from "../modules/toast";
import { refreshMeta, serverMeta } from "../stores/workspace";

const authRequired = ref(false);
const oldToken = ref("");
const newToken = ref("");
const saving = ref(false);

onMounted(async () => {
  await refreshMeta();
  const me = await api.me();
  authRequired.value = me.auth_required;
});

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
