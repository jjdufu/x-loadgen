<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
    <div class="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
      <span class="text-xs font-semibold tracking-tight text-foreground">
        最近历史
      </span>
      <div class="flex items-center gap-0.5">
        <UiButton
          variant="ghost"
          size="icon-xs"
          title="刷新"
          aria-label="刷新"
          :disabled="loading"
          @click="reload"
        >
          <RefreshCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" />
        </UiButton>
        <UiButton
          variant="ghost"
          size="icon-xs"
          title="完整历史"
          aria-label="完整历史"
          @click="router.push({ name: 'history' })"
        >
          <ExternalLink class="h-3.5 w-3.5" />
        </UiButton>
      </div>
    </div>

    <div class="space-y-2 border-b border-border p-2.5">
      <div
        class="grid grid-cols-2 gap-1 rounded-md border border-border bg-muted p-0.5"
        role="tablist"
        aria-label="历史类型"
      >
        <button
          type="button"
          role="tab"
          :aria-selected="mode === 'probe'"
          class="h-6 cursor-pointer rounded text-[11px] font-medium transition-all"
          :class="
            mode === 'probe'
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          "
          @click="mode = 'probe'"
        >
          单发
        </button>
        <button
          type="button"
          role="tab"
          :aria-selected="mode === 'run'"
          class="h-6 cursor-pointer rounded text-[11px] font-medium transition-all"
          :class="
            mode === 'run'
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          "
          @click="mode = 'run'"
        >
          压测
        </button>
      </div>

      <UiSearchInput
        v-model="searchText"
        placeholder="名称、URL"
        aria-label="搜索最近历史"
      />
    </div>

    <div class="flex-1 overflow-y-auto p-2">
      <div
        v-if="loading && currentCount === 0"
        class="flex items-center justify-center gap-2 py-12 text-xs text-muted-foreground"
      >
        <Loader2 class="h-3.5 w-3.5 animate-spin" />
        <span>加载中…</span>
      </div>

      <!-- Probe Quick List -->
      <template v-else-if="mode === 'probe'">
        <div
          v-if="filteredProbes.length === 0"
          class="flex flex-col items-center justify-center gap-2 py-12 text-center text-xs text-muted-foreground"
        >
          <History class="h-6 w-6 opacity-35" />
          <span>{{ searchText.trim() ? "无匹配记录" : "暂无单发历史" }}</span>
        </div>

        <div v-else class="space-y-1">
          <button
            v-for="item in filteredProbes"
            :key="item.id"
            type="button"
            :disabled="openingId === `p:${item.id}`"
            class="flex w-full cursor-pointer items-start gap-2 rounded-lg border border-transparent px-2.5 py-2 text-left transition-colors hover:border-border hover:bg-accent/50 disabled:opacity-60"
            @click="openProbe(item)"
          >
            <span
              class="mt-0.5 w-10 shrink-0 font-mono text-[10px] font-semibold"
              :class="methodColorClass(item.method)"
            >
              {{ item.method }}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                <span class="truncate text-xs font-medium text-foreground">
                  {{ item.request_name }}
                </span>
                <span
                  v-if="item.protocol === 'mazbri'"
                  class="shrink-0 rounded border border-border bg-muted px-1 font-mono text-[9px] text-muted-foreground"
                >
                  MZ
                </span>
              </div>
              <div class="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
                {{ item.url }}
              </div>
              <div class="mt-1 flex items-center gap-2 font-mono text-[10px] text-muted-foreground tabular-nums">
                <span
                  :class="
                    item.transport_ok && item.passed_assertions !== false
                      ? 'text-emerald-600 dark:text-emerald-400'
                      : 'text-rose-600 dark:text-rose-400'
                  "
                >
                  {{ item.http_status ?? (item.error_class || "ERR") }}
                </span>
                <span>{{ formatLatencyUs(item.latency_us) }}</span>
                <span class="ml-auto">{{ formatShortTime(item.created_at) }}</span>
              </div>
            </div>
            <Loader2
              v-if="openingId === `p:${item.id}`"
              class="mt-1 h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground"
            />
          </button>
        </div>
      </template>

      <!-- Load Run Quick List -->
      <template v-else>
        <div
          v-if="filteredRuns.length === 0"
          class="flex flex-col items-center justify-center gap-2 py-12 text-center text-xs text-muted-foreground"
        >
          <Activity class="h-6 w-6 opacity-35" />
          <span>{{ searchText.trim() ? "无匹配记录" : "暂无压测历史" }}</span>
        </div>

        <div v-else class="space-y-1">
          <div
            v-for="run in filteredRuns"
            :key="run.id"
            class="group flex items-start gap-2 rounded-lg border border-transparent px-2.5 py-2 transition-colors hover:border-border hover:bg-accent/50"
          >
            <button
              type="button"
              :disabled="openingId === `r:${run.id}`"
              class="min-w-0 flex-1 cursor-pointer text-left disabled:opacity-60"
              @click="rerunLoad(run)"
            >
              <div class="flex items-center gap-1.5">
                <span
                  class="h-1.5 w-1.5 shrink-0 rounded-full"
                  :class="runDotClass(run.status)"
                />
                <span class="truncate text-xs font-medium text-foreground">
                  {{ run.title || run.id.slice(0, 12) }}
                </span>
              </div>
              <div class="mt-1 flex items-center gap-2 font-mono text-[10px] text-muted-foreground tabular-nums">
                <span>{{ run.mode === "loop" ? "循环" : "单次" }}</span>
                <span v-if="run.achieved_qps != null">{{ run.achieved_qps.toFixed(1) }} QPS</span>
                <span v-else-if="run.current_qps">{{ run.current_qps }} QPS</span>
                <span class="ml-auto">{{ formatShortTime(run.created_at) }}</span>
              </div>
            </button>
            <UiButton
              variant="ghost"
              size="icon-xs"
              title="报告"
              aria-label="报告"
              class="shrink-0 opacity-60 group-hover:opacity-100"
              @click="router.push({ name: 'run-detail', params: { id: run.id } })"
            >
              <FileBarChart2 class="h-3.5 w-3.5" />
            </UiButton>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  Activity,
  ExternalLink,
  FileBarChart2,
  History,
  Loader2,
  RefreshCw,
} from "lucide-vue-next";
import { api } from "../../api/workspace";
import type { ProbeRecord, RunView } from "../../api/types";
import {
  formatLatencyUs,
  methodColorClass,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import {
  openProbeHistoryInNewTab,
  openRunReplayInWorkspace,
  probeHistoryVersion,
} from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiSearchInput from "../ui/UiSearchInput.vue";

const router = useRouter();
const mode = ref<"probe" | "run">("probe");
const searchText = ref("");
const loading = ref(false);
const openingId = ref<string | null>(null);
const probes = ref<ProbeRecord[]>([]);
const runs = ref<RunView[]>([]);

const filteredProbes = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  if (!q) return probes.value;
  return probes.value.filter(
    (p) =>
      p.request_name.toLowerCase().includes(q) ||
      p.url.toLowerCase().includes(q) ||
      p.method.toLowerCase().includes(q) ||
      (p.trace_id ?? "").toLowerCase().includes(q),
  );
});

const filteredRuns = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  if (!q) return runs.value;
  return runs.value.filter(
    (r) =>
      (r.title ?? "").toLowerCase().includes(q) ||
      r.id.toLowerCase().includes(q),
  );
});

const currentCount = computed(() =>
  mode.value === "probe" ? probes.value.length : runs.value.length,
);

function formatShortTime(tsMs: number): string {
  const d = new Date(tsMs);
  const now = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  if (
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate()
  ) {
    return `${hh}:${mm}:${ss}`;
  }
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const da = String(d.getDate()).padStart(2, "0");
  return `${mo}-${da} ${hh}:${mm}`;
}

function runDotClass(status: string): string {
  if (status === "running" || status === "starting" || status === "stopping") {
    return "bg-emerald-500";
  }
  if (status === "failed" || status === "interrupted") {
    return "bg-rose-500";
  }
  return "bg-muted-foreground/50";
}

async function reload(): Promise<void> {
  loading.value = true;
  try {
    const [p, r] = await Promise.all([
      api.listProbes({ limit: 60 }),
      api.listRuns(),
    ]);
    probes.value = p.items;
    runs.value = r.slice(0, 40);
  } finally {
    loading.value = false;
  }
}

async function openProbe(item: ProbeRecord): Promise<void> {
  openingId.value = `p:${item.id}`;
  try {
    const detail = await api.getProbe(item.id);
    openProbeHistoryInNewTab(detail);
  } catch {
    toast.error("历史记录加载失败");
  } finally {
    openingId.value = null;
  }
}

async function rerunLoad(run: RunView): Promise<void> {
  openingId.value = `r:${run.id}`;
  try {
    const payload = await api.getRunReplay(run.id);
    openRunReplayInWorkspace(payload);
    toast.success("已载入历史压测草稿");
  } catch {
    toast.error("压测配置加载失败");
  } finally {
    openingId.value = null;
  }
}

watch(probeHistoryVersion, () => {
  void reload();
});

onMounted(() => {
  void reload();
});
</script>
