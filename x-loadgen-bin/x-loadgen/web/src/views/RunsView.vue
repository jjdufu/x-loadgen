<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-5 p-6 lg:p-8">
      <!-- Top Filter & Shared Search Toolbar -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <!-- Segmented Status Filter -->
        <div class="inline-flex flex-wrap items-center rounded-lg border border-border bg-muted p-1 gap-0.5">
          <button
            v-for="f in filters"
            :key="f.value"
            type="button"
            class="h-7 rounded-md px-3 text-xs font-medium whitespace-nowrap transition-all"
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

        <div class="flex flex-wrap items-center gap-2">
          <UiSearchInput
            v-model="searchQuery"
            placeholder="搜索任务名称、目标或 ID..."
            wrapper-class="w-60 sm:w-64"
          />
          <UiButton variant="outline" size="sm" @click="loadRuns">
            <RefreshCw class="h-3.5 w-3.5" />
            <span>刷新</span>
          </UiButton>
          <UiButton
            variant="default"
            size="sm"
            @click="router.push({ name: 'workspace' })"
          >
            <Globe class="h-3.5 w-3.5" />
            <span>工作台</span>
          </UiButton>
        </div>
      </div>

      <!-- Empty State -->
      <UiCard
        v-if="filteredRuns.length === 0"
        class="flex flex-col items-center justify-center gap-3 border-dashed py-20 text-center"
      >
        <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-muted/50 text-muted-foreground">
          <Activity class="h-5 w-5" />
        </div>
        <div class="space-y-1">
          <p class="text-sm font-medium text-foreground">
            {{ searchQuery.trim() ? "未找到匹配的压测任务" : "暂无压测任务记录" }}
          </p>
          <p class="text-xs text-muted-foreground">
            {{
              searchQuery.trim()
                ? "请尝试调整搜索关键词或状态筛选条件。"
                : "在工作台中任意请求或集合上点击「压测」即可启动任务。"
            }}
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
                v-for="run in filteredRuns"
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
                  <UiBadge :variant="statusVariant(run.status)">
                    {{ formatRunStatus(run.status) }}
                  </UiBadge>
                </td>
                <td class="px-4 py-3.5 font-mono text-xs text-foreground tabular-nums">
                  {{ formatRunMode(run.mode) }} · {{ run.processes }} 进程
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
                      详情
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
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Activity, Globe, RefreshCw, Square, Trash2 } from "lucide-vue-next";
import { api } from "../api/workspace";
import type { RunView } from "../api/types";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSearchInput from "../components/ui/UiSearchInput.vue";
import {
  formatRunMode,
  formatRunStatus,
  formatTimeMs,
} from "../helpers/format";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";

const router = useRouter();
const runs = ref<RunView[]>([]);
const statusFilter = ref("");
const searchQuery = ref("");
let pollTimer: number | null = null;

const filters = [
  { value: "", label: "全部" },
  { value: "running", label: "运行中" },
  { value: "finished", label: "已完成" },
  { value: "stopped", label: "已停止" },
  { value: "failed", label: "失败" },
];

const filteredRuns = computed<RunView[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return runs.value;
  return runs.value.filter(
    (r) =>
      (r.title && r.title.toLowerCase().includes(q)) ||
      r.id.toLowerCase().includes(q) ||
      (r.target_id && r.target_id.toLowerCase().includes(q)) ||
      r.template_id.toLowerCase().includes(q),
  );
});

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
  const confirmed = await askConfirm({
    title: "删除压测任务",
    description: "确定删除此压测任务及全部明细样本吗？删除后无法恢复。",
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
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
