<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-y-auto p-6 space-y-5">
    <div class="flex flex-wrap items-center justify-between gap-3 border-b border-dividerLight pb-4">
      <div>
        <h1 class="heading">压测任务中心 (Load Test Runs)</h1>
        <p class="text-tiny text-secondaryLight mt-1">
          实时查看单接口或集合加权压测任务、动态在线调速、续跑中断任务并导出 JSON / HTML 离线报告。
        </p>
      </div>
      <div class="flex items-center space-x-2">
        <HoppButtonSecondary :icon="RefreshCw" label="刷新" outline @click="loadRuns" />
        <HoppButtonPrimary
          :icon="Globe"
          label="前往接口工作台发起压测"
          @click="router.push({ name: 'workspace' })"
        />
      </div>
    </div>

    <!-- Status Filter Pills -->
    <div class="flex flex-wrap items-center gap-1.5">
      <button
        v-for="f in filters"
        :key="f.value"
        type="button"
        class="px-3 py-1 rounded text-tiny font-semibold border transition"
        :class="
          statusFilter === f.value
            ? 'border-accent bg-accent/15 text-secondaryDark'
            : 'border-dividerLight text-secondary hover:text-secondaryDark'
        "
        @click="selectFilter(f.value)"
      >
        {{ f.label }}
      </button>
    </div>

    <div v-if="runs.length === 0" class="py-16 text-center border border-dashed border-divider rounded space-y-3">
      <Activity class="w-10 h-10 mx-auto text-accent opacity-40" />
      <p class="text-secondaryLight text-tiny">暂无符合筛选条件的压测任务记录</p>
    </div>

    <div v-else class="border border-dividerLight rounded overflow-x-auto">
      <table class="w-full text-left border-collapse text-tiny">
        <thead>
          <tr class="border-b border-dividerLight bg-primaryLight text-secondaryLight uppercase">
            <th class="px-4 py-2.5">任务 ID / 来源</th>
            <th class="px-4 py-2.5">状态</th>
            <th class="px-4 py-2.5">模式 / 进程</th>
            <th class="px-4 py-2.5">当前速率 / 并发</th>
            <th class="px-4 py-2.5">启动时间</th>
            <th class="px-4 py-2.5 text-right">操作</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-dividerLight">
          <tr
            v-for="run in runs"
            :key="run.id"
            class="hover:bg-primaryLight/40 transition cursor-pointer"
            @click="router.push({ name: 'run-detail', params: { id: run.id } })"
          >
            <td class="px-4 py-3">
              <div class="font-semibold text-secondaryDark">
                {{ run.title || run.target_id || "压测任务" }}
              </div>
              <div class="font-mono text-[11px] text-secondaryLight">
                {{ run.id.slice(0, 12) }} · {{ run.template_id }}
              </div>
            </td>
            <td class="px-4 py-3">
              <span
                class="px-2 py-0.5 rounded-full font-mono text-[11px] border"
                :class="statusBadgeClass(run.status)"
              >
                {{ run.status }}
              </span>
            </td>
            <td class="px-4 py-3 font-mono text-secondaryDark">
              {{ run.mode }} · {{ run.processes }}p
            </td>
            <td class="px-4 py-3 font-mono text-secondaryDark">
              {{ run.current_qps ? `${run.current_qps} QPS` : "闭环" }} /
              {{ run.current_concurrency ?? "-" }} 并发
            </td>
            <td class="px-4 py-3 font-mono text-secondaryLight">
              {{ formatTimeMs(run.started_at ?? run.created_at) }}
            </td>
            <td class="px-4 py-3 text-right" @click.stop>
              <div class="inline-flex items-center space-x-1">
                <HoppButtonSecondary
                  label="监控详情"
                  outline
                  @click="router.push({ name: 'run-detail', params: { id: run.id } })"
                />
                <HoppButtonSecondary
                  v-if="run.status === 'running'"
                  :icon="Square"
                  color="red"
                  outline
                  @click="stopRun(run.id)"
                />
                <HoppButtonSecondary
                  v-else
                  :icon="Trash2"
                  color="red"
                  @click="removeRun(run.id)"
                />
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { HoppButtonPrimary, HoppButtonSecondary } from "@hoppscotch/ui";
import { Activity, Globe, RefreshCw, Square, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { RunView } from "../api/types";
import { formatTimeMs } from "../helpers/format";
import { toast } from "../modules/toast";

const router = useRouter();
const runs = ref<RunView[]>([]);
const statusFilter = ref("");
let pollTimer: number | null = null;

const filters = [
  { value: "", label: "全部任务" },
  { value: "running", label: "运行中 (running)" },
  { value: "finished", label: "已完成 (finished)" },
  { value: "stopped", label: "已停止 (stopped)" },
  { value: "failed", label: "异常失败 (failed)" },
];

function statusBadgeClass(status: string): string {
  switch (status) {
    case "running":
    case "stopping":
      return "border-emerald-500/40 bg-emerald-500/10 text-emerald-400";
    case "finished":
      return "border-sky-500/40 bg-sky-500/10 text-sky-400";
    case "stopped":
    case "interrupted":
      return "border-amber-500/40 bg-amber-500/10 text-amber-400";
    default:
      return "border-rose-500/40 bg-rose-500/10 text-rose-400";
  }
}

async function loadRuns(): Promise<void> {
  runs.value = await api.listRuns(statusFilter.value || undefined);
}

function selectFilter(val: string): void {
  statusFilter.value = val;
  void loadRuns();
}

async function stopRun(id: string): Promise<void> {
  await api.stopRun(id);
  toast.info("已发送停止压测信号");
  await loadRuns();
}

async function removeRun(id: string): Promise<void> {
  if (!confirm("确定删除此压测任务及全部明细样本吗？")) return;
  await api.deleteRun(id);
  toast.success("任务已删除");
  await loadRuns();
}

onMounted(() => {
  void loadRuns();
  pollTimer = window.setInterval(() => {
    void loadRuns();
  }, 2500);
});

onBeforeUnmount(() => {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
