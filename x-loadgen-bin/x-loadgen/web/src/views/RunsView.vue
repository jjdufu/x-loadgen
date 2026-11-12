<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-6 lg:p-8">
      <!-- Page Header -->
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div class="space-y-1">
          <h1 class="text-2xl font-semibold tracking-tight text-foreground">
            压测任务中心
          </h1>
          <p class="text-sm text-muted-foreground">
            管理单接口与集合加权压测任务，支持在线热调速率、断点续跑及导出离线 JSON / HTML 报告。
          </p>
        </div>

        <div class="flex items-center gap-2">
          <UiButton variant="outline" @click="loadRuns">
            <RefreshCw class="h-4 w-4" />
            <span>刷新</span>
          </UiButton>
          <UiButton variant="default" @click="router.push({ name: 'workspace' })">
            <Globe class="h-4 w-4" />
            <span>前往工作台发起压测</span>
          </UiButton>
        </div>
      </div>

      <!-- Segmented Status Filter -->
      <div class="inline-flex flex-wrap items-center rounded-lg border border-border bg-muted p-1 gap-0.5">
        <button
          v-for="f in filters"
          :key="f.value"
          type="button"
          class="h-8 rounded-md px-3 text-xs font-medium whitespace-nowrap transition-all"
          :class="
            statusFilter === f.value
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          "
          @click="selectFilter(f.value)"
        >
          {{ f.label }}
        </button>
      </div>

      <!-- Empty State -->
      <UiCard
        v-if="runs.length === 0"
        class="flex flex-col items-center justify-center gap-3 border-dashed py-20 text-center"
      >
        <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground">
          <Activity class="h-5 w-5" />
        </div>
        <div class="space-y-1">
          <p class="text-sm font-medium text-foreground">暂无匹配的压测任务记录</p>
          <p class="text-xs text-muted-foreground">
            在接口工作台中任意请求或集合上点击「压测」即可启动高并发任务。
          </p>
        </div>
      </UiCard>

      <!-- Shadcn Runs Table -->
      <UiCard v-else class="overflow-hidden">
        <div class="overflow-x-auto">
          <table class="w-full border-collapse text-left text-sm">
            <thead>
              <tr class="border-b border-border bg-muted/40 text-xs font-medium text-muted-foreground">
                <th class="px-5 py-3">任务名称 / ID</th>
                <th class="px-4 py-3">状态</th>
                <th class="px-4 py-3">执行模式</th>
                <th class="px-4 py-3">目标速率 / 并发</th>
                <th class="px-4 py-3">启动时间</th>
                <th class="px-5 py-3 text-right">操作</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border">
              <tr
                v-for="run in runs"
                :key="run.id"
                class="cursor-pointer transition-colors hover:bg-muted/35"
                @click="router.push({ name: 'run-detail', params: { id: run.id } })"
              >
                <td class="px-5 py-3.5">
                  <div class="font-medium text-foreground">
                    {{ run.title || run.target_id || "压测任务" }}
                  </div>
                  <div class="mt-0.5 font-mono text-xs text-muted-foreground">
                    {{ run.id.slice(0, 12) }} · {{ run.template_id }}
                  </div>
                </td>
                <td class="px-4 py-3.5">
                  <UiBadge :variant="statusVariant(run.status)" class="font-mono">
                    {{ run.status }}
                  </UiBadge>
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-foreground tabular-nums">
                  {{ run.mode }} · {{ run.processes }} 进程
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-foreground tabular-nums">
                  {{ run.current_qps ? `${run.current_qps} QPS` : "闭环饱和" }} /
                  {{ run.current_concurrency ?? "-" }} 并发
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-muted-foreground tabular-nums">
                  {{ formatTimeMs(run.started_at ?? run.created_at) }}
                </td>
                <td class="px-5 py-3.5 text-right" @click.stop>
                  <div class="inline-flex items-center gap-1.5">
                    <UiButton
                      variant="outline"
                      size="sm"
                      @click="router.push({ name: 'run-detail', params: { id: run.id } })"
                    >
                      监控详情
                    </UiButton>
                    <UiButton
                      v-if="run.status === 'running'"
                      variant="outline"
                      size="icon-sm"
                      title="停止运行"
                      class="text-rose-600 dark:text-rose-400"
                      @click="stopRun(run.id)"
                    >
                      <Square class="h-3.5 w-3.5" />
                    </UiButton>
                    <UiButton
                      v-else
                      variant="ghost"
                      size="icon-sm"
                      title="删除任务"
                      class="hover:text-rose-600 dark:hover:text-rose-400"
                      @click="removeRun(run.id)"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </UiButton>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Activity, Globe, RefreshCw, Square, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { RunView } from "../api/types";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
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

function statusVariant(
  status: string,
): "success" | "secondary" | "warning" | "destructive" {
  switch (status) {
    case "running":
    case "stopping":
      return "success";
    case "finished":
      return "secondary";
    case "stopped":
    case "interrupted":
      return "warning";
    default:
      return "destructive";
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
