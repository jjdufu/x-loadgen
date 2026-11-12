<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-y-auto p-6 space-y-6">
    <div class="flex flex-wrap items-center justify-between gap-3 border-b border-dividerLight pb-4">
      <div>
        <h1 class="heading">可复用压测模板 (Load Profiles)</h1>
        <p class="text-tiny text-secondaryLight mt-1">
          压测模板与具体接口解耦：定义好恒定 QPS、闭环并发、爬坡预热与 SLA 阈值后，可在工作台对任意 HTTP 请求、Mazbri 网关请求或整个集合一键复用。
        </p>
      </div>
      <HoppButtonPrimary
        :icon="Plus"
        label="新建压测模板"
        @click="openEditor(null)"
      />
    </div>

    <div v-if="loadProfiles.length === 0" class="py-16 text-center border border-dashed border-divider rounded space-y-3">
      <Gauge class="w-10 h-10 mx-auto text-accent opacity-40" />
      <p class="text-secondaryLight text-tiny">暂无保存的压测模板，点击右上角创建常用压测规格</p>
      <HoppButtonSecondary :icon="Plus" label="创建首个压测模板" outline @click="openEditor(null)" />
    </div>

    <div v-else class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
      <div
        v-for="item in loadProfiles"
        :key="item.id"
        class="p-4 rounded border border-dividerLight bg-primaryLight/30 hover:border-accent transition flex flex-col justify-between space-y-4"
      >
        <div class="space-y-2">
          <div class="flex items-start justify-between gap-2">
            <div>
              <h3 class="font-bold text-secondaryDark">{{ item.name }}</h3>
              <p v-if="item.note" class="text-tiny text-secondaryLight mt-0.5">
                {{ item.note }}
              </p>
            </div>
            <span class="px-2 py-0.5 rounded bg-accent/15 text-accent font-mono text-tiny">
              {{ item.model === "open_loop" ? `${item.qps} QPS` : "闭环饱和" }}
            </span>
          </div>

          <div class="grid grid-cols-2 gap-2 pt-2 text-tiny font-mono text-secondary">
            <div>并发数: <strong class="text-secondaryDark">{{ item.concurrency }}</strong></div>
            <div>进程数: <strong class="text-secondaryDark">{{ item.processes }}</strong></div>
            <div>
              预算:
              <strong class="text-secondaryDark">
                {{ item.duration_secs ? `${item.duration_secs}s` : `${item.requests} 次` }}
              </strong>
            </div>
            <div>
              模式:
              <strong class="text-secondaryDark">
                {{ item.loop_enabled ? "循环窗口" : "单轮压测" }}
              </strong>
            </div>
          </div>
        </div>

        <div class="flex items-center justify-between pt-3 border-t border-dividerLight">
          <span class="text-[11px] text-secondaryLight font-mono">
            更新于 {{ formatTimeMs(item.updated_at) }}
          </span>
          <div class="flex items-center space-x-1">
            <HoppButtonSecondary :icon="Edit3" label="编辑" outline @click="openEditor(item)" />
            <HoppButtonSecondary :icon="Trash2" color="red" outline @click="removeProfile(item)" />
          </div>
        </div>
      </div>
    </div>

    <!-- Editor Modal -->
    <HoppSmartModal
      v-if="editorOpen"
      dialog
      :title="editingId ? '编辑压测模板' : '新建压测模板'"
      styles="sm:max-w-3xl"
      @close="editorOpen = false"
    >
      <template #body>
        <div class="space-y-4">
          <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                模板名称 (唯一标识) *
              </label>
              <input v-model="name" type="text" placeholder="例如 基准冒烟 50QPS / 峰值冲击 500QPS" class="input" />
            </div>
            <div>
              <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
                备注说明 (可选)
              </label>
              <input v-model="note" type="text" placeholder="适用场景说明" class="input" />
            </div>
          </div>

          <LoadConfigForm
            v-model="config"
            :assert-spec="assertSpec"
            @update:assert-spec="(v) => (assertSpec = v)"
          />
        </div>
      </template>

      <template #footer>
        <div class="flex justify-end space-x-2 w-full">
          <HoppButtonSecondary label="取消" outline @click="editorOpen = false" />
          <HoppButtonPrimary :loading="saving" label="保存模板" @click="saveProfile" />
        </div>
      </template>
    </HoppSmartModal>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartModal,
} from "@hoppscotch/ui";
import { Edit3, Gauge, Plus, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { AssertSpec, LoadConfig, LoadProfile } from "../api/types";
import LoadConfigForm from "../components/common/LoadConfigForm.vue";
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
    toast.error("请输入模板名称");
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
      toast.success("压测模板已更新");
    } else {
      await api.createLoadProfile(payload);
      toast.success("压测模板已创建");
    }
    await refreshLoadProfiles();
    editorOpen.value = false;
  } finally {
    saving.value = false;
  }
}

async function removeProfile(profile: LoadProfile): Promise<void> {
  if (!confirm(`确定删除压测模板「${profile.name}」吗？`)) return;
  await api.deleteLoadProfile(profile.id);
  await refreshLoadProfiles();
  toast.success("压测模板已删除");
}
</script>
