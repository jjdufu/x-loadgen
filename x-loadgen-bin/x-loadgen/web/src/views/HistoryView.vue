<template>
  <div class="flex min-h-0 w-full flex-1 flex-col overflow-hidden bg-background">
    <!-- Top Toolbar -->
    <div
      class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border bg-surface/50 px-4 py-3 sm:px-6"
    >
      <div class="flex flex-wrap items-center gap-2">
        <div
          class="inline-flex items-center gap-0.5 rounded-lg border border-border bg-muted p-1"
          role="group"
          aria-label="历史类型"
        >
          <button
            v-for="tab in kindFilters"
            :key="tab.value"
            type="button"
            class="h-7 cursor-pointer rounded-md px-3 text-xs font-medium whitespace-nowrap transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:scale-[0.98]"
            :class="
              kindFilter === tab.value
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'
            "
            @click="kindFilter = tab.value"
          >
            {{ tab.label }}
          </button>
        </div>

        <UiSelect
          v-model="protocolFilter"
          size="sm"
          wrapper-class="w-28"
          aria-label="协议筛选"
        >
          <option value="">全部协议</option>
          <option value="http">HTTP</option>
          <option value="mazbri">Mazbri</option>
        </UiSelect>
      </div>

      <div class="flex w-full flex-wrap items-center gap-2 sm:w-auto">
        <UiSearchInput
          v-model="searchText"
          placeholder="名称、URL、ID、trace_id"
          aria-label="搜索历史记录"
          wrapper-class="min-w-0 flex-1 sm:w-64 sm:flex-none"
        />
        <UiButton variant="outline" size="sm" :disabled="loading" @click="reloadAll">
          <RefreshCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" />
          <span>刷新</span>
        </UiButton>
        <UiButton
          v-if="probes.length > 0 && kindFilter !== 'run'"
          variant="outline"
          size="sm"
          @click="handleClearProbes"
        >
          <Trash2 class="h-3.5 w-3.5" />
          <span>清空单发</span>
        </UiButton>
      </div>
    </div>

    <!-- Main Split Pane -->
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <!-- Left Timeline List -->
      <div
        class="flex min-h-0 flex-col overflow-y-auto border-r border-border"
        :class="selected ? 'hidden w-full lg:flex lg:w-[440px] lg:shrink-0' : 'w-full lg:w-[440px] lg:shrink-0'"
      >
        <div
          v-if="loading && unifiedItems.length === 0"
          class="flex items-center justify-center gap-2 py-20 text-xs text-muted-foreground"
        >
          <Loader2 class="h-4 w-4 animate-spin" />
          <span>加载中…</span>
        </div>

        <div
          v-else-if="groupedItems.length === 0"
          class="flex flex-col items-center justify-center gap-3 px-6 py-20 text-center text-muted-foreground"
        >
          <History class="h-8 w-8 opacity-35" />
          <p class="text-xs">暂无历史记录</p>
          <UiButton variant="outline" size="sm" @click="router.push({ name: 'workspace' })">
            工作台
          </UiButton>
        </div>

        <div v-else class="divide-y divide-border/60 p-2.5">
          <div v-for="group in groupedItems" :key="group.day" class="pb-3 pt-1 first:pt-0">
            <div class="px-2 py-1.5 text-[11px] font-medium text-muted-foreground">
              {{ group.day }}
            </div>
            <div class="space-y-1">
              <button
                v-for="item in group.items"
                :key="item.key"
                type="button"
                class="flex w-full cursor-pointer items-center gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                :class="
                  selected?.key === item.key
                    ? 'border-border-hover bg-accent hover:bg-accent-hover'
                    : 'border-transparent hover:border-border-hover hover:bg-hover-bg active:bg-active-bg'
                "
                @click="selectItem(item)"
              >
                <template v-if="item.kind === 'probe'">
                  <span
                    class="w-12 shrink-0 font-mono text-[10px] font-semibold"
                    :class="methodColorClass(item.probe.method)"
                  >
                    {{ item.probe.method }}
                  </span>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-1.5">
                      <span class="truncate text-xs font-medium text-foreground">
                        {{ item.probe.request_name }}
                      </span>
                      <span
                        v-if="item.probe.protocol === 'mazbri'"
                        class="shrink-0 rounded border border-border bg-muted px-1 font-mono text-[9px] text-foreground"
                      >
                        MZ
                      </span>
                    </div>
                    <div class="truncate font-mono text-[10px] text-muted-foreground">
                      {{ item.probe.url }}
                    </div>
                  </div>
                  <div class="shrink-0 text-right">
                    <span
                      class="inline-block rounded border px-1 font-mono text-[10px] tabular-nums"
                      :class="statusCategoryClass(item.probe.http_status, item.probe.transport_ok)"
                    >
                      {{ item.probe.http_status ?? item.probe.error_class ?? "ERR" }}
                    </span>
                    <div class="mt-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">
                      {{ timeOf(item.created_at) }}
                    </div>
                  </div>
                </template>

                <template v-else>
                  <span
                    class="w-12 shrink-0 rounded border border-violet-500/30 bg-violet-500/10 py-0.5 text-center font-mono text-[10px] font-semibold text-violet-700 dark:text-violet-300"
                  >
                    压测
                  </span>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-1.5">
                      <span class="truncate text-xs font-medium text-foreground">
                        {{ item.run.title || item.run.target_id || "压测任务" }}
                      </span>
                      <span
                        v-for="p in item.run.protocols ?? []"
                        :key="p"
                        class="shrink-0 rounded border border-border bg-muted px-1 font-mono text-[9px] uppercase text-muted-foreground"
                      >
                        {{ p === "mazbri" ? "MZ" : p }}
                      </span>
                    </div>
                    <div class="truncate font-mono text-[10px] text-muted-foreground tabular-nums">
                      {{ item.run.samples ?? 0 }} 样本 ·
                      {{ item.run.achieved_qps != null ? `${item.run.achieved_qps.toFixed(1)} QPS` : formatRate(item.run) }}
                      <template v-if="item.run.p95_us != null">
                        · P95 {{ formatLatencyUs(item.run.p95_us) }}
                      </template>
                    </div>
                  </div>
                  <div class="shrink-0 text-right">
                    <UiBadge
                      :variant="runStatusBadgeVariant(item.run.status)"
                      class="px-1.5 py-0 text-[10px]"
                    >
                      {{ formatRunStatus(item.run.status) }}
                    </UiBadge>
                    <div class="mt-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">
                      {{ timeOf(item.created_at) }}
                    </div>
                  </div>
                </template>
              </button>
            </div>
          </div>

          <div v-if="nextBeforeId !== null && kindFilter !== 'run'" class="flex justify-center py-3">
            <UiButton variant="outline" size="xs" :disabled="loadingMore" @click="loadMoreProbes">
              {{ loadingMore ? "加载中…" : "加载更多单发" }}
            </UiButton>
          </div>
        </div>
      </div>

      <!-- Right Detail Panel -->
      <div
        class="flex min-h-0 flex-1 flex-col overflow-y-auto bg-background p-4 sm:p-6"
        :class="selected ? 'flex' : 'hidden lg:flex'"
      >
        <div
          v-if="!selected"
          class="flex flex-1 flex-col items-center justify-center gap-2 text-center text-muted-foreground"
        >
          <History class="h-10 w-10 opacity-25" />
          <p class="text-xs">选择左侧记录查看完整参数与响应</p>
        </div>

        <!-- PROBE DETAIL -->
        <div v-else-if="selected.kind === 'probe'" class="mx-auto w-full max-w-4xl space-y-5">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0 space-y-1">
              <div class="flex items-center gap-2">
                <UiButton
                  variant="ghost"
                  size="icon-xs"
                  class="lg:hidden"
                  aria-label="返回列表"
                  @click="selected = null"
                >
                  <ArrowLeft class="h-4 w-4" />
                </UiButton>
                <span
                  class="font-mono text-xs font-semibold"
                  :class="methodColorClass(selected.probe.method)"
                >
                  {{ selected.probe.method }}
                </span>
                <UiBadge variant="outline" class="font-mono uppercase text-[10px]">
                  {{ selected.probe.protocol }}
                </UiBadge>
                <h2 class="truncate text-base font-semibold text-foreground">
                  {{ selected.probe.request_name }}
                </h2>
              </div>
              <div class="break-all font-mono text-xs text-muted-foreground">
                {{ selected.probe.url }}
              </div>
            </div>

            <div class="flex flex-wrap items-center gap-2">
              <UiButton
                variant="default"
                size="sm"
                :disabled="detailLoading"
                @click="reopenProbe"
              >
                <RotateCcw class="h-3.5 w-3.5" />
                <span>重发编辑</span>
              </UiButton>
              <UiButton variant="outline" size="sm" @click="removeProbe(selected.probe)">
                <Trash2 class="h-3.5 w-3.5" />
                <span>删除</span>
              </UiButton>
            </div>
          </div>

          <!-- Status & Telemetry Strip -->
          <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">状态</div>
              <div class="mt-1 font-mono text-sm font-semibold">
                {{ selected.probe.http_status ?? selected.probe.error_class ?? "ERR" }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">耗时</div>
              <div class="mt-1 font-mono text-sm font-semibold tabular-nums">
                {{ formatLatencyUs(selected.probe.latency_us) }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">响应体积</div>
              <div class="mt-1 font-mono text-sm font-semibold tabular-nums">
                {{ formatBytes(selected.probe.response_bytes) }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">时间</div>
              <div class="mt-1 font-mono text-xs tabular-nums text-foreground">
                {{ formatTimeMs(selected.probe.created_at) }}
              </div>
            </UiCard>
          </div>

          <div
            v-if="detailLoading"
            class="flex items-center gap-2 py-8 text-xs text-muted-foreground"
          >
            <Loader2 class="h-4 w-4 animate-spin" />
            <span>加载详情…</span>
          </div>

          <template v-else-if="probeDetail">
            <!-- Request Snapshot (with secrets) -->
            <UiCard class="space-y-3 p-4">
              <div class="flex items-center justify-between">
                <h3 class="text-xs font-semibold text-foreground">请求快照（含密钥）</h3>
                <div class="flex items-center gap-2 font-mono text-[11px] text-muted-foreground">
                  <span v-if="probeDetail.environment_name">环境 {{ probeDetail.environment_name }}</span>
                  <span v-if="probeDetail.tenant_name">租户 {{ probeDetail.tenant_name }}</span>
                </div>
              </div>
              <div class="flex flex-wrap gap-3 font-mono text-[11px] text-muted-foreground">
                <button
                  type="button"
                  class="cursor-pointer rounded px-1 py-0.5 transition-colors hover:bg-hover-bg hover:text-foreground"
                  @click="copy(probeDetail.request_id, 'X-Request-Id')"
                >
                  X-Request-Id: <span class="text-foreground">{{ probeDetail.request_id }}</span>
                </button>
                <button
                  type="button"
                  class="cursor-pointer rounded px-1 py-0.5 transition-colors hover:bg-hover-bg hover:text-foreground"
                  @click="copy(probeDetail.trace_id, 'trace_id')"
                >
                  trace_id: <span class="text-foreground">{{ probeDetail.trace_id }}</span>
                </button>
                <UiBadge :variant="probeDetail.passed_assertions ? 'success' : 'destructive'">
                  {{ probeDetail.passed_assertions ? "断言通过" : "断言未通过" }}
                </UiBadge>
                <UiBadge :variant="traceEchoVariant(probeDetail.trace_echo)">
                  {{ traceEchoLabel(probeDetail.trace_echo) }}
                </UiBadge>
              </div>

              <div v-if="probeDetail.request_draft" class="space-y-2 pt-1">
                <div class="flex flex-wrap gap-2 text-[11px]">
                  <UiBadge variant="muted" class="font-mono">
                    Auth: {{ probeDetail.request_draft.auth?.auth_type || "none" }}
                  </UiBadge>
                  <UiBadge
                    v-if="probeDetail.request_draft.headers?.length"
                    variant="muted"
                    class="font-mono"
                  >
                    Headers: {{ probeDetail.request_draft.headers.filter((h) => h.active).length }}
                  </UiBadge>
                  <UiBadge
                    v-if="probeDetail.request_draft.params?.length"
                    variant="muted"
                    class="font-mono"
                  >
                    Params: {{ probeDetail.request_draft.params.filter((p) => p.active).length }}
                  </UiBadge>
                  <UiBadge
                    v-if="probeDetail.request_draft.body?.body"
                    variant="muted"
                    class="font-mono"
                  >
                    Body: {{ probeDetail.request_draft.body.content_type || "raw" }}
                  </UiBadge>
                </div>
                <CodeMirrorEditor
                  :model-value="JSON.stringify(probeDetail.request_draft, null, 2)"
                  language="json"
                  readonly
                  min-height="120px"
                  max-height="240px"
                />
              </div>
            </UiCard>

            <!-- Response Headers & Body -->
            <UiCard class="space-y-3 p-4">
              <div class="flex items-center justify-between">
                <h3 class="text-xs font-semibold text-foreground">响应详情</h3>
                <span v-if="probeDetail.response_truncated" class="font-mono text-[11px] text-amber-600">
                  响应体已截断（最大 64KB）
                </span>
              </div>
              <div
                v-if="probeDetail.error"
                class="rounded border border-rose-500/30 bg-rose-500/10 px-3 py-2 font-mono text-xs text-rose-600 dark:text-rose-400"
              >
                {{ probeDetail.error }}
              </div>
              <div
                v-if="probeDetail.response_headers && probeDetail.response_headers.length"
                class="max-h-36 overflow-y-auto rounded border border-border bg-muted/30 p-2.5 font-mono text-[11px]"
              >
                <div
                  v-for="([k, v], i) in probeDetail.response_headers"
                  :key="`${k}-${i}`"
                  class="flex gap-2 py-0.5"
                >
                  <span class="shrink-0 font-medium text-muted-foreground">{{ k }}:</span>
                  <span class="break-all text-foreground">{{ v }}</span>
                </div>
              </div>
              <pre
                class="max-h-96 overflow-auto whitespace-pre-wrap break-all rounded border border-border bg-muted/40 p-3 font-mono text-xs text-foreground"
              >{{ probeDetail.response_body || "（空响应体）" }}</pre>
            </UiCard>
          </template>
        </div>

        <!-- LOAD-TEST RUN DETAIL -->
        <div v-else class="mx-auto w-full max-w-4xl space-y-5">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0 space-y-1">
              <div class="flex flex-wrap items-center gap-2">
                <UiButton
                  variant="ghost"
                  size="icon-xs"
                  class="lg:hidden"
                  aria-label="返回列表"
                  @click="selected = null"
                >
                  <ArrowLeft class="h-4 w-4" />
                </UiButton>
                <UiBadge :variant="runStatusBadgeVariant(selected.run.status)">
                  {{ formatRunStatus(selected.run.status) }}
                </UiBadge>
                <UiBadge
                  v-if="selected.run.verdict_result"
                  :variant="selected.run.verdict_result === 'pass' ? 'success' : selected.run.verdict_result === 'fail' ? 'destructive' : 'warning'"
                >
                  SLA {{ formatVerdictResult(selected.run.verdict_result) }}
                </UiBadge>
                <h2 class="truncate text-base font-semibold text-foreground">
                  {{ selected.run.title || selected.run.target_id || "压测任务" }}
                </h2>
              </div>
              <div class="font-mono text-xs text-muted-foreground">
                ID {{ selected.run.id }} · {{ formatTimeMs(selected.run.started_at ?? selected.run.created_at) }}
              </div>
            </div>

            <div class="flex flex-wrap items-center gap-2">
              <UiButton
                variant="default"
                size="sm"
                :loading="replayingId === selected.run.id"
                @click="handleRerun(selected.run.id)"
              >
                <RotateCcw v-if="replayingId !== selected.run.id" class="h-3.5 w-3.5" />
                <span>重跑</span>
              </UiButton>
              <UiButton
                variant="outline"
                size="sm"
                @click="router.push({ name: 'run-detail', params: { id: selected.run.id } })"
              >
                <BarChart3 class="h-3.5 w-3.5" />
                <span>查看报告</span>
              </UiButton>
              <UiButton
                v-if="selected.run.status !== 'running' && selected.run.status !== 'stopping'"
                variant="outline"
                size="sm"
                @click="removeRun(selected.run)"
              >
                <Trash2 class="h-3.5 w-3.5" />
                <span>删除</span>
              </UiButton>
            </div>
          </div>

          <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">样本数</div>
              <div class="mt-1 font-mono text-sm font-semibold tabular-nums">
                {{ selected.run.samples ?? 0 }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">平均 QPS</div>
              <div class="mt-1 font-mono text-sm font-semibold tabular-nums">
                {{ selected.run.achieved_qps != null ? selected.run.achieved_qps.toFixed(1) : "-" }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">P95 / P99</div>
              <div class="mt-1 font-mono text-sm font-semibold tabular-nums">
                {{ formatLatencyUs(selected.run.p95_us) }} / {{ formatLatencyUs(selected.run.p99_us) }}
              </div>
            </UiCard>
            <UiCard class="p-3">
              <div class="text-[11px] text-muted-foreground">失败请求</div>
              <div
                class="mt-1 font-mono text-sm font-semibold tabular-nums"
                :class="(selected.run.failure ?? 0) > 0 ? 'text-rose-600 dark:text-rose-400' : ''"
              >
                {{ selected.run.failure ?? 0 }}
              </div>
            </UiCard>
          </div>

          <div
            v-if="detailLoading"
            class="flex items-center gap-2 py-8 text-xs text-muted-foreground"
          >
            <Loader2 class="h-4 w-4 animate-spin" />
            <span>加载配置快照…</span>
          </div>

          <UiCard v-else-if="runReplayDetail" class="space-y-4 p-4">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="text-xs font-semibold text-foreground">压测配置快照（含密钥）</h3>
              <div class="flex flex-wrap gap-2 font-mono text-[11px] text-muted-foreground">
                <span>{{ formatRunMode(runReplayDetail.mode) }}</span>
                <span>{{ runReplayDetail.load_config.model === "open_loop" ? `${runReplayDetail.load_config.qps ?? "-"} QPS` : "闭环" }}</span>
                <span>{{ runReplayDetail.load_config.concurrency }} 并发</span>
                <span>{{ runReplayDetail.load_config.processes }} 进程</span>
                <span v-if="runReplayDetail.environment_name">环境 {{ runReplayDetail.environment_name }}</span>
              </div>
            </div>

            <div class="space-y-2">
              <div
                v-for="(req, idx) in runReplayDetail.requests"
                :key="idx"
                class="flex items-center justify-between gap-2 rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs"
              >
                <div class="flex min-w-0 items-center gap-2">
                  <UiBadge variant="outline" class="uppercase text-[10px]">
                    {{ req.protocol }}
                  </UiBadge>
                  <span class="font-semibold" :class="methodColorClass(req.method)">
                    {{ req.method }}
                  </span>
                  <span class="truncate text-foreground">{{ req.name }}</span>
                  <span class="truncate text-muted-foreground">{{ req.endpoint }}</span>
                </div>
                <span class="shrink-0 text-[11px] text-muted-foreground">
                  权重 {{ req.weight }}
                </span>
              </div>
            </div>

            <CodeMirrorEditor
              :model-value="JSON.stringify({ load_config: runReplayDetail.load_config, assert: runReplayDetail.assert, requests: runReplayDetail.requests }, null, 2)"
              language="json"
              readonly
              min-height="160px"
              max-height="360px"
            />
          </UiCard>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  ArrowLeft,
  BarChart3,
  History,
  Loader2,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import type { ProbeRecord, RunReplayPayload, RunView } from "../api/types";
import CodeMirrorEditor from "../components/common/CodeMirrorEditor.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSearchInput from "../components/ui/UiSearchInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import {
  formatBytes,
  formatLatencyUs,
  formatRunMode,
  formatRunStatus,
  formatTimeMs,
  formatVerdictResult,
  methodColorClass,
  runStatusBadgeVariant,
  statusCategoryClass,
  traceEchoLabel,
  traceEchoVariant,
} from "../helpers/format";
import { askConfirm } from "../modules/confirm";
import { toast } from "../modules/toast";
import {
  openProbeHistoryInNewTab,
  openRunReplayInWorkspace,
} from "../stores/workspace";
import { copyText } from "../utils/clipboard";

type UnifiedItem =
  | { key: string; kind: "probe"; created_at: number; probe: ProbeRecord }
  | { key: string; kind: "run"; created_at: number; run: RunView };

const router = useRouter();
const kindFilters = [
  { value: "all" as const, label: "全部" },
  { value: "probe" as const, label: "单发" },
  { value: "run" as const, label: "压测" },
];

const kindFilter = ref<"all" | "probe" | "run">("all");
const protocolFilter = ref("");
const searchText = ref("");
const loading = ref(false);
const loadingMore = ref(false);
const detailLoading = ref(false);
const replayingId = ref<string | null>(null);

const probes = ref<ProbeRecord[]>([]);
const nextBeforeId = ref<number | null>(null);
const runs = ref<RunView[]>([]);

const selected = ref<UnifiedItem | null>(null);
const probeDetail = ref<ProbeRecord | null>(null);
const runReplayDetail = ref<RunReplayPayload | null>(null);

const unifiedItems = computed<UnifiedItem[]>(() => {
  const q = searchText.value.trim().toLowerCase();
  const proto = protocolFilter.value;
  const out: UnifiedItem[] = [];

  if (kindFilter.value === "all" || kindFilter.value === "probe") {
    for (const p of probes.value) {
      if (proto && p.protocol !== proto) continue;
      if (
        q &&
        ![p.request_name, p.url, p.method, p.request_id, p.trace_id].some((f) =>
          f.toLowerCase().includes(q),
        )
      ) {
        continue;
      }
      out.push({
        key: `probe-${p.id}`,
        kind: "probe",
        created_at: p.created_at,
        probe: p,
      });
    }
  }

  if (kindFilter.value === "all" || kindFilter.value === "run") {
    for (const r of runs.value) {
      if (proto && !(r.protocols ?? []).includes(proto)) continue;
      if (
        q &&
        ![r.title ?? "", r.id, r.target_id, r.template_id, r.environment_name ?? ""].some(
          (f) => f.toLowerCase().includes(q),
        )
      ) {
        continue;
      }
      out.push({
        key: `run-${r.id}`,
        kind: "run",
        created_at: r.started_at ?? r.created_at,
        run: r,
      });
    }
  }

  out.sort((a, b) => b.created_at - a.created_at);
  return out;
});

const groupedItems = computed(() => {
  const groups: { day: string; items: UnifiedItem[] }[] = [];
  for (const item of unifiedItems.value) {
    const day = new Date(item.created_at).toLocaleDateString("zh-CN");
    const last = groups[groups.length - 1];
    if (last && last.day === day) {
      last.items.push(item);
    } else {
      groups.push({ day, items: [item] });
    }
  }
  return groups;
});

function timeOf(ms: number): string {
  return new Date(ms).toLocaleTimeString("zh-CN", { hour12: false });
}

function formatRate(run: RunView): string {
  if (run.current_qps && run.current_qps > 0) return `${run.current_qps} QPS`;
  return run.model === "closed_loop" ? "闭环" : "不限速";
}

async function reloadAll(): Promise<void> {
  loading.value = true;
  try {
    const [probePage, runList] = await Promise.all([
      api.listProbes({ limit: 80 }),
      api.listRuns(),
    ]);
    probes.value = probePage.items;
    nextBeforeId.value = probePage.next_before_id;
    runs.value = runList;
  } finally {
    loading.value = false;
  }
}

async function loadMoreProbes(): Promise<void> {
  if (nextBeforeId.value === null) return;
  loadingMore.value = true;
  try {
    const page = await api.listProbes({ limit: 80, before_id: nextBeforeId.value });
    probes.value = [...probes.value, ...page.items];
    nextBeforeId.value = page.next_before_id;
  } finally {
    loadingMore.value = false;
  }
}

async function selectItem(item: UnifiedItem): Promise<void> {
  selected.value = item;
  probeDetail.value = null;
  runReplayDetail.value = null;
  detailLoading.value = true;
  try {
    if (item.kind === "probe") {
      probeDetail.value = await api.getProbe(item.probe.id);
    } else {
      runReplayDetail.value = await api.getRunReplay(item.run.id);
    }
  } finally {
    detailLoading.value = false;
  }
}

async function reopenProbe(): Promise<void> {
  if (!probeDetail.value) return;
  openProbeHistoryInNewTab(probeDetail.value);
  await router.push({ name: "workspace" });
  toast.success("已载入独立草稿");
}

async function handleRerun(runId: string): Promise<void> {
  replayingId.value = runId;
  try {
    const replay = runReplayDetail.value?.run_id === runId
      ? runReplayDetail.value
      : await api.getRunReplay(runId);
    openRunReplayInWorkspace(replay);
    await router.push({ name: "workspace" });
    toast.success("已载入历史配置");
  } finally {
    replayingId.value = null;
  }
}

async function removeProbe(probe: ProbeRecord): Promise<void> {
  await api.deleteProbe(probe.id);
  probes.value = probes.value.filter((p) => p.id !== probe.id);
  if (selected.value?.key === `probe-${probe.id}`) {
    selected.value = null;
    probeDetail.value = null;
  }
  toast.success("已删除");
}

async function removeRun(run: RunView): Promise<void> {
  const confirmed = await askConfirm({
    title: "删除压测记录",
    description: "将同时删除该压测报告及全部明细样本，删除后无法恢复。",
    confirmText: "删除",
    variant: "destructive",
  });
  if (!confirmed) return;
  await api.deleteRun(run.id);
  runs.value = runs.value.filter((r) => r.id !== run.id);
  if (selected.value?.key === `run-${run.id}`) {
    selected.value = null;
    runReplayDetail.value = null;
  }
  toast.success("已删除");
}

async function handleClearProbes(): Promise<void> {
  const confirmed = await askConfirm({
    title: "清空单发历史",
    description: "确定删除全部单发历史记录吗？此操作无法恢复。",
    confirmText: "清空",
    variant: "destructive",
  });
  if (!confirmed) return;
  await api.clearProbes();
  probes.value = [];
  nextBeforeId.value = null;
  if (selected.value?.kind === "probe") {
    selected.value = null;
    probeDetail.value = null;
  }
  toast.success("已清空");
}

async function copy(value: string, label: string): Promise<void> {
  if (await copyText(value)) toast.success(`已复制 ${label}`);
}

onMounted(() => {
  void reloadAll();
});
</script>
