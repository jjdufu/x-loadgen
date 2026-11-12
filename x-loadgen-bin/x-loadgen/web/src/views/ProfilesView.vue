<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-6 lg:p-8">
      <!-- Header -->
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div class="space-y-1">
          <h1 class="text-2xl font-semibold tracking-tight text-foreground">
            可复用负载预设
          </h1>
          <p class="text-sm text-muted-foreground">
            将常用的 QPS、并发、爬坡预热及 SLA 判定规则保存为独立预设，对任意 HTTP 请求、Mazbri 网关接口或集合一键复用。
          </p>
        </div>

        <UiButton variant="default" @click="openEditor(null)">
          <Plus class="h-4 w-4" />
          <span>新建负载预设</span>
        </UiButton>
      </div>

      <!-- Empty State -->
      <UiCard
        v-if="loadProfiles.length === 0"
        class="flex flex-col items-center justify-center gap-3 border-dashed py-20 text-center"
      >
        <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground">
          <Gauge class="h-5 w-5" />
        </div>
        <div class="space-y-1">
          <p class="text-sm font-medium text-foreground">暂无已保存的负载预设</p>
          <p class="text-xs text-muted-foreground">
            点击右上角「新建负载预设」创建基准冒烟、容量评估或峰值冲击规格。
          </p>
        </div>
        <UiButton variant="outline" size="sm" @click="openEditor(null)">
          <Plus class="h-3.5 w-3.5" />
          <span>创建首个负载预设</span>
        </UiButton>
      </UiCard>

      <!-- Profile Cards Grid -->
      <div v-else class="grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3">
        <UiCard
          v-for="item in loadProfiles"
          :key="item.id"
          class="flex flex-col justify-between p-5 space-y-5 transition-colors hover:border-foreground/30"
        >
          <div class="space-y-3">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <h3 class="truncate text-base font-semibold text-foreground">
                  {{ item.name }}
                </h3>
                <p v-if="item.note" class="mt-0.5 text-xs text-muted-foreground">
                  {{ item.note }}
                </p>
              </div>
              <UiBadge variant="secondary" class="font-mono shrink-0">
                {{ item.model === "open_loop" ? `${item.qps} QPS` : "闭环饱和" }}
              </UiBadge>
            </div>

            <div class="grid grid-cols-2 gap-2.5 rounded-lg border border-border bg-surface/50 p-3 font-mono text-xs tabular-nums">
              <div class="text-muted-foreground">
                最大并发: <strong class="text-foreground">{{ item.concurrency }}</strong>
              </div>
              <div class="text-muted-foreground">
                进程数: <strong class="text-foreground">{{ item.processes }}</strong>
              </div>
              <div class="text-muted-foreground">
                预算:
                <strong class="text-foreground">
                  {{ item.duration_secs ? `${item.duration_secs}s` : `${item.requests} 次` }}
                </strong>
              </div>
              <div class="text-muted-foreground">
                模式:
                <strong class="text-foreground">
                  {{ item.loop_enabled ? "循环窗口" : "单轮压测" }}
                </strong>
              </div>
            </div>
          </div>

          <div class="flex items-center justify-between border-t border-border pt-3.5">
            <span class="font-mono text-[11px] text-muted-foreground tabular-nums">
              更新于 {{ formatTimeMs(item.updated_at) }}
            </span>
            <div class="flex items-center gap-1.5">
              <UiButton variant="outline" size="sm" @click="openEditor(item)">
                <Edit3 class="h-3.5 w-3.5" />
                <span>编辑</span>
              </UiButton>
              <UiButton
                variant="ghost"
                size="icon-sm"
                title="删除预设"
                class="hover:text-rose-600 dark:hover:text-rose-400"
                @click="removeProfile(item)"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </UiButton>
            </div>
          </div>
        </UiCard>
      </div>
    </div>

    <!-- Editor Dialog -->
    <UiDialog
      :open="editorOpen"
      :title="editingId ? '编辑负载预设' : '新建负载预设'"
      description="配置发压模型、预算、集群进程数及可选的 SLA 自动判定阈值。"
      max-width="3xl"
      @close="editorOpen = false"
    >
      <div class="space-y-5">
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              预设名称 (唯一标识) *
            </label>
            <UiInput
              v-model="name"
              placeholder="例如 基准冒烟 50QPS / 峰值冲击 500QPS"
            />
          </div>
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              备注说明 (可选)
            </label>
            <UiInput v-model="note" placeholder="适用场景说明" />
          </div>
        </div>

        <LoadConfigForm
          v-model="config"
          :assert-spec="assertSpec"
          @update:assert-spec="(v) => (assertSpec = v)"
        />
      </div>

      <template #footer>
        <UiButton variant="outline" @click="editorOpen = false">取消</UiButton>
        <UiButton variant="default" :loading="saving" @click="saveProfile">
          保存预设
        </UiButton>
      </template>
    </UiDialog>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Edit3, Gauge, Plus, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { AssertSpec, LoadConfig, LoadProfile } from "../api/types";
import LoadConfigForm from "../components/common/LoadConfigForm.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiDialog from "../components/ui/UiDialog.vue";
import UiInput from "../components/ui/UiInput.vue";
import {
  cloneJson,
  createDefaultLoadConfig,
  formatTimeMs,
} from "../helpers/format";
import { toast } from "../modules/toast";
import { loadProfiles, refreshLoadProfiles } from "../stores/workspace";

const editorOpen = ref(false);
const editingId = ref<string | null>(null);
const name = ref("");
const note = ref("");
const config = ref<LoadConfig>(createDefaultLoadConfig());
const assertSpec = ref<AssertSpec | null>(null);
const saving = ref(false);

onMounted(() => {
  void refreshLoadProfiles();
});

function openEditor(profile: LoadProfile | null): void {
  if (!profile) {
    editingId.value = null;
    name.value = "";
    note.value = "";
    config.value = createDefaultLoadConfig();
    assertSpec.value = null;
  } else {
    editingId.value = profile.id;
    name.value = profile.name;
    note.value = profile.note ?? "";
    config.value = {
      model: profile.model,
      qps: profile.qps ?? null,
      concurrency: profile.concurrency,
      requests: profile.requests ?? null,
      duration_secs: profile.duration_secs ?? null,
      warmup_secs: profile.warmup_secs,
      ramp_secs: profile.ramp_secs,
      timeout_secs: profile.timeout_secs,
      connect_timeout_secs: profile.connect_timeout_secs,
      token_refresh_skew_secs: profile.token_refresh_skew_secs,
      include_request_id: profile.include_request_id,
      processes: profile.processes,
      qps_mode: profile.qps_mode,
      requests_mode: profile.requests_mode,
      loop_enabled: profile.loop_enabled,
      rounds: profile.rounds ?? null,
      loop_interval_secs: profile.loop_interval_secs,
      expected_reasons: cloneJson(profile.expected_reasons ?? []),
      detail_retain_days: profile.detail_retain_days ?? null,
      detail_retain_rows: profile.detail_retain_rows ?? null,
    };
    assertSpec.value = profile.assert ? cloneJson(profile.assert) : null;
  }
  editorOpen.value = true;
}

async function saveProfile(): Promise<void> {
  if (!name.value.trim()) {
    toast.error("请输入预设名称");
    return;
  }
  saving.value = true;
  try {
    const payload = {
      name: name.value.trim(),
      note: note.value.trim() || null,
      ...config.value,
      assert: assertSpec.value,
    };
    if (editingId.value) {
      await api.updateLoadProfile(editingId.value, payload);
      toast.success("负载预设已更新");
    } else {
      await api.createLoadProfile(payload);
      toast.success("负载预设已创建");
    }
    await refreshLoadProfiles();
    editorOpen.value = false;
  } finally {
    saving.value = false;
  }
}

async function removeProfile(profile: LoadProfile): Promise<void> {
  if (!confirm(`确定删除负载预设「${profile.name}」吗？`)) return;
  await api.deleteLoadProfile(profile.id);
  await refreshLoadProfiles();
  toast.success("负载预设已删除");
}
</script>
