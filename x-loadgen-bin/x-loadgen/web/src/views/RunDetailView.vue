<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 lg:p-8">
      <div
        v-if="loadError && run"
        role="alert"
        class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-300"
      >
        <span>任务数据加载失败：{{ loadError }}</span>
        <UiButton variant="outline" size="sm" @click="loadRun">重试</UiButton>
      </div>
      <div
        v-else-if="refreshError && run"
        role="status"
        class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-300 bg-amber-50 px-4 py-2.5 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300"
      >
        <span class="min-w-0 break-words">
          数据刷新失败，当前显示的可能不是最新数据{{ isRunning ? "（正在自动重试）" : "" }}：{{ refreshError }}
        </span>
        <UiButton variant="outline" size="sm" @click="retryRefresh">立即重试</UiButton>
      </div>

      <!-- Top Header & Controls -->
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-0 space-y-1.5">
          <div class="flex flex-wrap items-center gap-2.5">
            <UiButton
              variant="outline"
              size="icon-sm"
              aria-label="返回压测任务列表"
              @click="router.push({ name: 'runs' })"
            >
              <ArrowLeft class="h-4 w-4" />
            </UiButton>
            <h1 class="min-w-0 break-words text-lg font-semibold tracking-tight text-foreground sm:text-xl">
              {{ run?.title || "压测任务详情" }}
            </h1>
            <UiBadge
              v-if="run"
              :variant="runStatusBadgeVariant(run.status)"
            >
              {{ formatRunStatus(run.status) }}
            </UiBadge>
            <UiBadge
              v-if="run?.verdict && run.verdict.result !== 'none'"
              :variant="verdictVariant(run.verdict.result)"
            >
              SLA: {{ formatVerdictResult(run.verdict.result) }}
            </UiBadge>
          </div>
          <p
            class="flex flex-wrap gap-x-3 gap-y-0.5 font-mono text-xs text-muted-foreground tabular-nums sm:pl-10"
          >
            <span class="break-all">ID: {{ id }}</span>
            <template v-if="run">
              <span class="whitespace-nowrap">模式: {{ formatRunMode(run.mode) }}</span>
              <span class="whitespace-nowrap">工作进程: {{ run.processes ?? 1 }}</span>
            </template>
          </p>
        </div>

        <div v-if="run" class="flex flex-wrap items-center gap-2">
          <UiButton
            v-if="isRunning"
            variant="destructive"
            size="sm"
            :disabled="actionBusy !== null || run.status === 'stopping'"
            @click="handleStop"
          >
            <Square class="h-3.5 w-3.5" />
            <span>{{ actionBusy === "stop" || run.status === "stopping" ? "停止中…" : "停止" }}</span>
          </UiButton>
          <UiButton
            v-if="canResume"
            variant="default"
            size="sm"
            :disabled="actionBusy !== null"
            @click="handleResume"
          >
            <Play class="h-3.5 w-3.5" />
            <span>{{ actionBusy === "resume" ? "恢复中…" : "续跑" }}</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            :disabled="exportingKind !== null"
            @click="downloadReport('json')"
          >
            <Download class="h-3.5 w-3.5" />
            <span>{{ exportingKind === "json" ? "导出中…" : "JSON 报告" }}</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            :disabled="exportingKind !== null"
            @click="downloadReport('html')"
          >
            <FileCode2 class="h-3.5 w-3.5" />
            <span>{{ exportingKind === "html" ? "导出中…" : "HTML 报告" }}</span>
          </UiButton>
        </div>
      </div>

      <!-- Not-found / Load Error State -->
      <UiCard
        v-if="loadError && !run"
        role="alert"
        class="space-y-4 p-8 text-center"
      >
        <div class="space-y-1.5">
          <h2 class="text-base font-semibold text-foreground">
            无法加载该压测任务
          </h2>
          <p class="mx-auto max-w-md break-words text-sm text-muted-foreground">
            {{ loadError }}
          </p>
        </div>
        <div class="flex flex-wrap items-center justify-center gap-2.5">
          <UiButton variant="outline" size="sm" @click="router.push({ name: 'runs' })">
            返回任务列表
          </UiButton>
          <UiButton variant="default" size="sm" @click="loadRun">
            重新加载
          </UiButton>
        </div>
      </UiCard>

      <!-- Initial Loading State -->
      <UiCard
        v-else-if="!run"
        class="p-10 text-center text-sm text-muted-foreground"
      >
        正在加载任务数据…
      </UiCard>

      <template v-else>
        <!-- Error Banner -->
        <div
          v-if="run.error"
          class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-4 py-3 font-mono text-xs text-rose-700 dark:text-rose-400"
        >
          任务错误：{{ run.error }}
        </div>

        <!-- Shadcn Dashboard KPI Metric Cards -->
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-4 sm:gap-3.5 xl:grid-cols-8">
          <UiCard
            v-for="card in metricCards"
            :key="card.label"
            class="min-w-0 space-y-1.5 p-3 sm:p-4"
          >
            <div class="text-xs font-medium text-muted-foreground whitespace-nowrap">
              {{ card.label }}
            </div>
            <div
              class="truncate font-mono text-lg font-semibold tracking-tight tabular-nums sm:text-xl"
              :class="card.colorClass ?? 'text-foreground'"
              :title="String(card.value)"
            >
              {{ card.value }}
            </div>
          </UiCard>
        </div>

        <!-- Live Traffic Tuner Card (shown only while live) -->
        <UiCard v-if="isRunning" class="p-4">
          <form
            class="flex flex-wrap items-end gap-4"
            @submit.prevent="applyTrafficPatch"
          >
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                动态目标 QPS
              </label>
              <UiInput
                :model-value="tuneQps"
                type="number"
                min="1"
                mono
                :disabled="actionBusy === 'tune'"
                class="w-36"
                @update:model-value="(v) => (tuneQps = Math.max(1, Number(v) || 1))"
              />
            </div>
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                动态并发上限
              </label>
              <UiInput
                :model-value="tuneConcurrency"
                type="number"
                min="1"
                mono
                :disabled="actionBusy === 'tune'"
                class="w-36"
                @update:model-value="(v) => (tuneConcurrency = Math.max(1, Number(v) || 1))"
              />
            </div>
            <UiButton
              type="submit"
              variant="default"
              :disabled="actionBusy !== null"
            >
              {{ actionBusy === "tune" ? "下发中…" : "应用" }}
            </UiButton>
            <span class="pb-2 text-xs text-muted-foreground">
              运行期间可随时调整 QPS 与并发，实时同步至全部工作进程。
            </span>
          </form>
        </UiCard>

        <!-- ECharts Live Throughput Series -->
        <UiCard class="p-5 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h2 class="text-sm font-semibold text-foreground">
                每秒吞吐量与成功 / 失败趋势
              </h2>
              <p class="text-xs text-muted-foreground">
                按秒聚合全集群样本速率
              </p>
            </div>
          </div>
          <div class="relative h-72 w-full">
            <div ref="chartEl" class="h-full w-full" />
            <div
              v-if="lastSeries.length === 0"
              class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
            >
              暂无秒级吞吐采样数据
            </div>
          </div>
        </UiCard>

        <!-- Bottom Detailed Explorer Card -->
        <UiCard class="overflow-hidden">
          <div
            class="flex h-11 items-center gap-1 overflow-x-auto border-b border-border bg-surface/50 px-4 select-none"
            role="tablist"
          >
            <button
              v-for="tab in detailTabs"
              :key="tab.id"
              type="button"
              role="tab"
              :aria-selected="activeDetailTab === tab.id"
              class="relative inline-flex h-full cursor-pointer items-center gap-1.5 px-3 text-xs font-medium whitespace-nowrap transition-colors"
              :class="
                activeDetailTab === tab.id
                  ? 'text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              "
              @click="activeDetailTab = tab.id"
            >
              <span>{{ tab.label }}</span>
              <span
                v-if="tab.count"
                class="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-foreground tabular-nums"
              >
                {{ tab.count }}
              </span>
              <span
                v-if="activeDetailTab === tab.id"
                class="absolute inset-x-2 bottom-0 h-[2px] rounded-full bg-foreground"
              />
            </button>
          </div>

          <!-- 1. Samples Explorer -->
          <div v-if="activeDetailTab === 'samples'" class="space-y-4 p-4 sm:p-5">
            <div class="flex flex-wrap items-end gap-3 border-b border-border pb-4">
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  结果状态
                </label>
                <UiSelect v-model="sampleFilterSuccess" size="sm" wrapper-class="w-36" aria-label="结果状态">
                  <option value="all">全部样本</option>
                  <option value="false">仅失败请求</option>
                  <option value="true">仅成功请求</option>
                </UiSelect>
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  错误分类
                </label>
                <UiSelect
                  v-model="sampleFilterErrClass"
                  size="sm"
                  wrapper-class="w-44"
                  aria-label="错误分类"
                >
                  <option value="">全部分类</option>
                  <option v-for="c in ERROR_CLASSES" :key="c.value" :value="c.value">
                    {{ c.label }}
                  </option>
                </UiSelect>
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  最低延迟 (ms)
                </label>
                <UiInput
                  v-model="sampleFilterMinMs"
                  type="number"
                  min="0"
                  size="sm"
                  mono
                  placeholder="0"
                  aria-label="最低延迟 (ms)"
                  class="w-28"
                />
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  租户
                </label>
                <UiInput
                  v-model="sampleFilterTenant"
                  size="sm"
                  mono
                  placeholder="精确匹配"
                  aria-label="租户"
                  class="w-36"
                />
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  请求号 / 链路号
                </label>
                <UiInput
                  v-model="sampleFilterId"
                  size="sm"
                  mono
                  placeholder="X-Request-Id 或 trace_id"
                  aria-label="请求号或链路号"
                  class="w-72"
                />
              </div>
              <UiButton variant="outline" size="sm" :disabled="sampleLoading" @click="loadSamples('search')">
                检索
              </UiButton>
            </div>

            <div
              v-if="samples.length === 0"
              class="py-10 text-center text-sm text-muted-foreground"
            >
              暂无匹配的请求明细样本
            </div>

            <div v-else class="overflow-x-auto">
              <table class="w-full border-collapse whitespace-nowrap text-left font-mono text-xs">
                <thead>
                  <tr class="border-b border-border text-muted-foreground">
                    <th class="py-2.5 pr-4 font-medium">进程#序号</th>
                    <th class="py-2.5 pr-4 font-medium">开始时间</th>
                    <th class="py-2.5 pr-4 font-medium">接口</th>
                    <th class="py-2.5 pr-4 font-medium">租户</th>
                    <th class="py-2.5 pr-4 font-medium">状态</th>
                    <th class="py-2.5 pr-4 font-medium">延迟</th>
                    <th class="py-2.5 pr-4 font-medium">字节</th>
                    <th class="py-2.5 pr-4 font-medium">请求号 / 链路号</th>
                    <th class="py-2.5 font-medium">错误 / 诊断信息</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border">
                  <template v-for="s in samples" :key="`${s.worker}-${s.seq}`">
                    <tr
                      class="hover:bg-muted/35"
                      :class="s.response_body ? 'cursor-pointer' : ''"
                      @click="toggleSampleBody(s)"
                    >
                      <td class="py-2 pr-4 text-muted-foreground tabular-nums">
                        w{{ s.worker }}#{{ s.seq }}
                      </td>
                      <td class="py-2 pr-4 text-muted-foreground tabular-nums">
                        {{ formatSampleTime(s.start_unix_ms) }}
                      </td>
                      <td class="py-2 pr-4 text-foreground">
                        {{ s.product_name || s.product_id }}
                      </td>
                      <td class="py-2 pr-4 text-muted-foreground">
                        {{ s.customer_name || "-" }}
                      </td>
                      <td class="py-2 pr-4">
                        <span
                          class="font-semibold"
                          :class="
                            s.success
                              ? 'text-emerald-600 dark:text-emerald-400'
                              : 'text-rose-600 dark:text-rose-400'
                          "
                        >
                          {{ s.http_status ?? (s.success ? "成功" : "失败") }}
                        </span>
                      </td>
                      <td class="py-2 pr-4 text-foreground tabular-nums">
                        {{ formatLatencyUs(s.latency_us) }}
                      </td>
                      <td class="py-2 pr-4 text-muted-foreground tabular-nums">
                        {{ formatBytes(s.response_bytes) }}
                      </td>
                      <td class="py-2 pr-4 text-[11px] leading-tight text-muted-foreground">
                        <div class="select-all" :title="s.request_id ?? ''">{{ s.request_id || "-" }}</div>
                        <div
                          class="select-all"
                          :class="s.trace_echo === 'mismatch' ? 'text-amber-600 dark:text-amber-400' : ''"
                          :title="traceTitle(s)"
                        >
                          {{ s.trace_id || "-" }}
                        </div>
                      </td>
                      <td class="max-w-md truncate py-2 text-rose-600 dark:text-rose-400">
                        <span v-if="s.response_body" class="mr-1 text-muted-foreground">
                          {{ expandedSamples.has(sampleKey(s)) ? "▾" : "▸" }}
                        </span>
                        {{ s.error_class ? `[${s.error_class}] ` : "" }}{{ s.error ?? "" }}
                      </td>
                    </tr>
                    <tr v-if="s.response_body && expandedSamples.has(sampleKey(s))">
                      <td colspan="9" class="bg-muted/30 px-3 py-2">
                        <div class="mb-1 text-[11px] text-muted-foreground">
                          响应体（失败样本，最多保留 2 KB，敏感值已打码）
                        </div>
                        <pre class="max-h-64 overflow-auto whitespace-pre-wrap break-all text-[11px] text-foreground">{{ s.response_body }}</pre>
                      </td>
                    </tr>
                  </template>
                </tbody>
              </table>
            </div>

            <div v-if="sampleNextCursor" class="flex justify-center pt-2">
              <UiButton variant="outline" size="sm" :disabled="sampleLoading" @click="loadSamples('more')">
                {{ sampleLoading ? "加载中…" : "加载更多" }}
              </UiButton>
            </div>
          </div>

          <!-- 1b. Breakdown (from the report) -->
          <div v-else-if="activeDetailTab === 'breakdown'" class="space-y-5 p-5 text-xs">
            <div v-if="!breakdown" class="py-6 text-center text-sm text-muted-foreground">
              分项统计在压测结束后根据已落库样本生成
            </div>
            <template v-else>
              <p class="text-muted-foreground">
                基于 {{ breakdown.samples }} 条已落库样本（不含预热）；耗时为原始线上耗时，不做协调遗漏校正。
              </p>
              <div
                v-for="section in breakdownSections"
                :key="section.title"
                class="overflow-x-auto"
              >
                <div class="mb-2 font-medium text-foreground">{{ section.title }}</div>
                <table class="w-full border-collapse text-left font-mono tabular-nums">
                  <thead>
                    <tr class="border-b border-border text-muted-foreground">
                      <th class="py-2 pr-4 font-medium">{{ section.label }}</th>
                      <th class="py-2 pr-4 font-medium">样本</th>
                      <th class="py-2 pr-4 font-medium">成功</th>
                      <th class="py-2 pr-4 font-medium">失败</th>
                      <th class="py-2 pr-4 font-medium">预期拒绝</th>
                      <th class="py-2 pr-4 font-medium">p50</th>
                      <th class="py-2 pr-4 font-medium">p95</th>
                      <th class="py-2 pr-4 font-medium">p99</th>
                      <th class="py-2 font-medium">max</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    <tr v-for="g in section.rows" :key="g.product_id ?? g.tenant">
                      <td class="py-2 pr-4 text-foreground">{{ g.product_id ?? g.tenant }}</td>
                      <td class="py-2 pr-4">{{ g.samples }}</td>
                      <td class="py-2 pr-4 text-emerald-600 dark:text-emerald-400">{{ g.success }}</td>
                      <td class="py-2 pr-4 text-rose-600 dark:text-rose-400">{{ g.failure }}</td>
                      <td class="py-2 pr-4">{{ g.expected }}</td>
                      <td class="py-2 pr-4">{{ formatLatencyUs(g.p50_us) }}</td>
                      <td class="py-2 pr-4">{{ formatLatencyUs(g.p95_us) }}</td>
                      <td class="py-2 pr-4">{{ formatLatencyUs(g.p99_us) }}</td>
                      <td class="py-2">{{ formatLatencyUs(g.max_us) }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div>
                <div class="mb-2 font-medium text-foreground">HTTP 状态码分布</div>
                <div class="flex flex-wrap gap-2 font-mono">
                  <span
                    v-for="[code, n] in Object.entries(breakdown.status_codes)"
                    :key="code"
                    class="rounded border border-border px-2 py-1"
                  >
                    {{ code === "none" ? "无响应" : code }} × {{ n }}
                  </span>
                </div>
              </div>
              <div class="overflow-x-auto">
                <div class="mb-2 font-medium text-foreground">错误 Top {{ breakdown.top_errors.length }}</div>
                <div v-if="breakdown.top_errors.length === 0" class="text-muted-foreground">
                  无失败样本
                </div>
                <table v-else class="w-full border-collapse text-left font-mono">
                  <thead>
                    <tr class="border-b border-border text-muted-foreground">
                      <th class="py-2 pr-4 font-medium">分类</th>
                      <th class="py-2 pr-4 font-medium">错误</th>
                      <th class="py-2 pr-4 font-medium">次数</th>
                      <th class="py-2 font-medium">状态码</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    <tr v-for="e in breakdown.top_errors" :key="`${e.error_class}|${e.error}`">
                      <td class="py-2 pr-4 text-foreground">{{ e.error_class }}</td>
                      <td class="max-w-xl break-all py-2 pr-4 text-rose-600 dark:text-rose-400">{{ e.error }}</td>
                      <td class="py-2 pr-4 tabular-nums">{{ e.count }}</td>
                      <td class="py-2 tabular-nums">
                        {{ Object.entries(e.http_status).map(([c, n]) => `${c}×${n}`).join(" ") }}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
          </div>

          <!-- 2. SLA Verdict -->
          <div v-else-if="activeDetailTab === 'verdict'" class="p-5 space-y-4 text-sm">
            <div
              v-if="!run.verdict || run.verdict.result === 'none'"
              class="py-6 text-muted-foreground"
            >
              当前压测未配置 SLA 阈值判定规则。
            </div>
            <template v-else>
              <div class="flex items-center gap-2 font-medium text-foreground">
                <span>综合结论：</span>
                <UiBadge :variant="verdictVariant(run.verdict.result)">
                  {{ formatVerdictResult(run.verdict.result) }}
                </UiBadge>
              </div>
              <div v-if="run.verdict.failed?.length" class="space-y-2">
                <div
                  v-for="fc in run.verdict.failed"
                  :key="fc.check"
                  class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-4 py-2.5 font-mono text-xs text-rose-700 dark:text-rose-400"
                >
                  未达标项 {{ fc.check }}：目标阈值 {{ fc.wanted }}，实测值 {{ fc.actual ?? "-" }}
                </div>
              </div>
              <pre
                v-if="run.verdict.actual"
                class="rounded-lg border border-border bg-muted/40 p-4 font-mono text-xs text-foreground"
              >{{ JSON.stringify(run.verdict.actual, null, 2) }}</pre>
            </template>
          </div>

          <!-- 3. Loop Windows -->
          <div v-else-if="activeDetailTab === 'windows'" class="overflow-x-auto p-5">
            <div
              v-if="windows.length === 0"
              class="py-6 text-center text-sm text-muted-foreground"
            >
              当前任务无多轮循环窗口记录
            </div>
            <table v-else class="w-full border-collapse text-left font-mono text-xs tabular-nums">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="py-2.5 pr-4 font-medium">轮次 #</th>
                  <th class="py-2.5 pr-4 font-medium">样本数</th>
                  <th class="py-2.5 pr-4 font-medium">成功</th>
                  <th class="py-2.5 pr-4 font-medium">失败</th>
                  <th class="py-2.5 pr-4 font-medium">p50</th>
                  <th class="py-2.5 pr-4 font-medium">p95</th>
                  <th class="py-2.5 font-medium">p99</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr v-for="w in windows" :key="w.window_index">
                  <td class="py-2 pr-4 text-foreground">#{{ w.window_index }}</td>
                  <td class="py-2 pr-4 text-foreground">{{ w.samples }}</td>
                  <td class="py-2 pr-4 text-emerald-600 dark:text-emerald-400">
                    {{ w.success }}
                  </td>
                  <td class="py-2 pr-4 text-rose-600 dark:text-rose-400">
                    {{ w.failure }}
                  </td>
                  <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p50_us) }}</td>
                  <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p95_us) }}</td>
                  <td class="py-2 text-foreground">{{ formatLatencyUs(w.p99_us) }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- 4. Lifecycle Timeline -->
          <div
            v-else-if="activeDetailTab === 'timeline'"
            class="divide-y divide-border p-5 text-xs"
          >
            <div
              v-if="formattedTimeline.length === 0"
              class="py-6 text-center text-sm text-muted-foreground"
            >
              暂无生命周期事件记录
            </div>
            <div
              v-for="ev in formattedTimeline"
              :key="ev.id"
              class="flex flex-wrap items-center justify-between gap-2 py-2.5"
            >
              <div class="min-w-0">
                <span class="mr-3 font-medium text-foreground">{{ ev.label }}</span>
                <span v-if="ev.detail" class="text-muted-foreground">
                  {{ ev.detail }}
                </span>
              </div>
              <span class="font-mono text-muted-foreground tabular-nums">
                {{ ev.time }}
              </span>
            </div>
          </div>

          <!-- 5. Redacted Snapshot -->
          <div v-else class="p-5">
            <CodeMirrorEditor
              :model-value="snapshotJson"
              language="json"
              readonly
              min-height="220px"
              max-height="480px"
            />
          </div>
        </UiCard>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { init as initChart, use as useECharts, type EChartsType } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import {
  ArrowLeft,
  Download,
  FileCode2,
  Play,
  Square,
} from "lucide-vue-next";

useECharts([
  LineChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  CanvasRenderer,
]);
import { api } from "../api/workspace";
import type {
  RunStats,
  RunView,
  SampleCursor,
  SampleFilters,
  ReportBreakdown,
  SampleRow,
  TimePoint,
  TimelineEvent,
  WindowRow,
} from "../api/types";
import CodeMirrorEditor from "../components/common/CodeMirrorEditor.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import { downloadBlob } from "../helpers/download";
import {
  formatBytes,
  formatLatencyUs,
  formatRunMode,
  formatRunStatus,
  formatTimelineEvent,
  formatTimeMs,
  formatVerdictResult,
  runStatusBadgeVariant,
} from "../helpers/format";
import { isPageVisible } from "../modules/activity";
import { askConfirm } from "../modules/confirm";
import { effectiveTheme } from "../modules/theme";
import { toast } from "../modules/toast";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id ?? ""));

const run = ref<RunView | null>(null);
const stats = ref<RunStats | null>(null);
const windows = ref<WindowRow[]>([]);
const timeline = ref<TimelineEvent[]>([]);
const samples = ref<SampleRow[]>([]);
const lastSeries = ref<TimePoint[]>([]);
const sampleNextCursor = ref<SampleCursor | null>(null);
/** Filters the current sample list was fetched with; "load more" reuses them. */
let appliedSampleFilters: SampleFilters = {};
const sampleLoading = ref(false);
const actionBusy = ref<"stop" | "resume" | "tune" | null>(null);
const exportingKind = ref<"json" | "html" | null>(null);

const ERROR_CLASSES = [
  { value: "transport", label: "transport 传输" },
  { value: "business", label: "business 业务断言" },
  { value: "unexpected_accept", label: "unexpected_accept 意外放行" },
  { value: "protocol", label: "protocol 协议解析" },
  { value: "crypto", label: "crypto 加解密" },
  { value: "token_refresh", label: "token_refresh 取 token" },
  { value: "config", label: "config 配置" },
] as const;

const sampleFilterSuccess = ref<"all" | "true" | "false">("all");
const sampleFilterErrClass = ref("");
const sampleFilterMinMs = ref("");
const sampleFilterTenant = ref("");
/** One box for both ids: 32 hex chars is a trace id, anything else a request id. */
const sampleFilterId = ref("");
const expandedSamples = ref(new Set<string>());
const breakdown = ref<ReportBreakdown | null>(null);
const breakdownSections = computed(() =>
  breakdown.value
    ? [
        { title: "按接口 / 产品", label: "接口", rows: breakdown.value.products },
        { title: "按租户", label: "租户", rows: breakdown.value.tenants },
      ]
    : [],
);

function sampleKey(s: SampleRow): string {
  return `${s.worker}-${s.seq}`;
}

function toggleSampleBody(s: SampleRow) {
  if (!s.response_body) return;
  const next = new Set(expandedSamples.value);
  const key = sampleKey(s);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  expandedSamples.value = next;
}

function formatSampleTime(ms: number): string {
  if (!ms) return "-";
  const d = new Date(ms);
  return `${d.toLocaleTimeString("zh-CN", { hour12: false })}.${String(d.getMilliseconds()).padStart(3, "0")}`;
}

function traceTitle(s: SampleRow): string {
  switch (s.trace_echo) {
    case "match":
      return "响应 traceparent 与发送一致";
    case "mismatch":
      return "响应 traceparent 的 trace_id 与发送不一致";
    default:
      return "响应未回传 traceparent";
  }
}

const tuneQps = ref(20);
const tuneConcurrency = ref(10);
const activeDetailTab = ref<
  "samples" | "breakdown" | "verdict" | "windows" | "timeline" | "snapshot"
>("samples");

const chartEl = ref<HTMLDivElement | null>(null);
let chart: EChartsType | null = null;
let chartResizeObserver: ResizeObserver | null = null;
let sse: EventSource | null = null;
let pollTimer: number | null = null;
let initializedInputs = false;
/** Set once the view unmounts; async work that resolves later must not touch it. */
let disposed = false;
/** Bumped whenever the shown run changes; stale responses are dropped. */
let loadGeneration = 0;
let refreshInFlight: Promise<void> | null = null;
let refreshQueued = false;
let lastFullRefreshAt = 0;
/** Full refreshes (5 endpoints) while live; SSE ticks only patch stats in between. */
const FULL_REFRESH_INTERVAL_MS = 5000;
const FALLBACK_POLL_MS = 2000;
const loadError = ref<string | null>(null);
/** Last background refresh failure; cleared by the next successful refresh. */
const refreshError = ref<string | null>(null);
/**
 * Background and follow-up fetches never toast: a flaky server would
 * otherwise raise up to 5 toasts per tick. Errors surface inline instead
 * (`loadError` for the initial load, `refreshError` afterwards).
 */
const SILENT = { silent: true } as const;

function isLiveStatus(status: string | undefined): boolean {
  return status === "running" || status === "stopping";
}

const isRunning = computed(() => isLiveStatus(run.value?.status));
const canResume = computed(
  () => run.value?.status === "stopped" || run.value?.status === "interrupted",
);
const snapshotJson = computed(() =>
  JSON.stringify(run.value?.snapshot ?? {}, null, 2),
);
const formattedTimeline = computed(() =>
  timeline.value.map((ev) => {
    const formatted = formatTimelineEvent(ev.event, ev.payload);
    return {
      id: ev.id,
      label: formatted.label,
      detail: formatted.detail,
      time: formatTimeMs(ev.ts),
    };
  }),
);

const detailTabs = computed(() => [
  { id: "samples" as const, label: "样本明细", count: samples.value.length },
  { id: "breakdown" as const, label: "分项统计", count: 0 },
  { id: "verdict" as const, label: "SLA 判定", count: 0 },
  { id: "windows" as const, label: "循环窗口", count: windows.value.length },
  { id: "timeline" as const, label: "事件时间线", count: timeline.value.length },
  { id: "snapshot" as const, label: "执行快照", count: 0 },
]);

const metricCards = computed(() => {
  const s = stats.value;
  return [
    { label: "样本总数", value: s?.samples ?? 0 },
    {
      label: "成功请求",
      value: s?.success ?? 0,
      colorClass: "text-emerald-600 dark:text-emerald-400",
    },
    {
      label: "失败请求",
      value: s?.failure ?? 0,
      colorClass:
        (s?.failure ?? 0) > 0
          ? "text-rose-600 dark:text-rose-400"
          : "text-foreground",
    },
    { label: "p50 延迟", value: formatLatencyUs(s?.p50_us) },
    { label: "p95 延迟", value: formatLatencyUs(s?.p95_us) },
    { label: "p99 延迟", value: formatLatencyUs(s?.p99_us) },
    { label: "在途并发", value: s?.in_flight ?? 0 },
    { label: "累计字节", value: formatBytes(s?.bytes) },
  ];
});

function verdictVariant(result: string): "success" | "destructive" | "warning" {
  if (result === "pass") return "success";
  if (result === "fail") return "destructive";
  return "warning";
}

/**
 * Reload run, stats, windows, timeline and series. Concurrent calls coalesce
 * into one trailing refresh so slow responses never pile up or land out of
 * order; responses for a run that is no longer shown are discarded.
 */
async function refreshAll(): Promise<void> {
  if (refreshInFlight) {
    refreshQueued = true;
    return refreshInFlight;
  }
  const generation = loadGeneration;
  refreshInFlight = (async () => {
    try {
      do {
        refreshQueued = false;
        await fetchAll();
      } while (refreshQueued && !disposed);
      if (generation === loadGeneration) refreshError.value = null;
    } catch (err) {
      if (!disposed && generation === loadGeneration) {
        refreshError.value = err instanceof Error ? err.message : String(err);
      }
      throw err;
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

async function fetchAll(): Promise<void> {
  const generation = loadGeneration;
  lastFullRefreshAt = Date.now();
  const [runData, statsData, winData, timeData, seriesData] = await Promise.all([
    api.getRun(id.value, SILENT),
    api.getRunStats(id.value, SILENT),
    api.getRunWindows(id.value, SILENT),
    api.getRunTimeline(id.value, SILENT),
    api.getRunTimeseries(id.value, SILENT),
  ]);
  if (disposed || generation !== loadGeneration) return;
  const wasLive = isLiveStatus(run.value?.status);
  const firstMountOfDashboard = run.value === null;
  run.value = runData;
  stats.value = statsData;
  windows.value = winData;
  timeline.value = timeData;

  let effectiveSeries = seriesData;
  if (
    !isLiveStatus(runData.status) &&
    statsData.samples > 0 &&
    (effectiveSeries.length === 0 || !breakdown.value)
  ) {
    try {
      const report = await api.getRunReport(id.value, SILENT);
      if (disposed || generation !== loadGeneration) return;
      if (effectiveSeries.length === 0) effectiveSeries = report.timeseries ?? [];
      breakdown.value = report.breakdown ?? null;
    } catch {
      // Keep empty series / breakdown if the report is still finalizing
    }
  }
  lastSeries.value = effectiveSeries;

  if (!initializedInputs) {
    if (statsData.current_qps) tuneQps.value = statsData.current_qps;
    if (statsData.current_concurrency) {
      tuneConcurrency.value = statsData.current_concurrency;
    }
    initializedInputs = true;
  }
  if (firstMountOfDashboard) {
    await nextTick();
    if (disposed || generation !== loadGeneration) return;
  }
  renderChart(effectiveSeries);
  syncLiveStream(wasLive);
}

/** Open the live stream while the run is live; close it once it ends. */
function syncLiveStream(wasLive: boolean): void {
  if (disposed) return;
  const live = isLiveStatus(run.value?.status);
  if (live && !sse && pollTimer === null) {
    startLiveStream();
  } else if (!live && (sse || pollTimer !== null)) {
    stopLiveStream();
  }
  if (wasLive && !live) {
    // Final samples are flushed when the run ends.
    void loadSamples("reload").catch(() => {});
  }
}

/** Apply an SSE `stats` tick: patch the live numbers without refetching. */
function applyStatsTick(raw: string): void {
  let payload: Partial<RunStats>;
  try {
    payload = JSON.parse(raw) as Partial<RunStats>;
  } catch {
    return;
  }
  if (payload.id && payload.id !== id.value) return;
  if (stats.value) {
    stats.value = { ...stats.value, ...payload };
  }
  const statusChanged = Boolean(
    payload.status && run.value && payload.status !== run.value.status,
  );
  if (
    statusChanged ||
    Date.now() - lastFullRefreshAt >= FULL_REFRESH_INTERVAL_MS
  ) {
    void refreshAll().catch(() => {});
  }
}

function readSampleFilters(): SampleFilters {
  const minMs = Number(sampleFilterMinMs.value);
  return {
    success:
      sampleFilterSuccess.value === "all"
        ? undefined
        : sampleFilterSuccess.value === "true",
    error_class: sampleFilterErrClass.value || undefined,
    tenant: sampleFilterTenant.value.trim() || undefined,
    ...idFilter(sampleFilterId.value.trim()),
    min_latency_us:
      sampleFilterMinMs.value !== "" && Number.isFinite(minMs) && minMs > 0
        ? Math.round(minMs * 1000)
        : undefined,
  };
}

function idFilter(raw: string): Pick<SampleFilters, "request_id" | "trace_id"> {
  if (!raw) return {};
  return /^[0-9a-fA-F]{32}$/.test(raw) ? { trace_id: raw } : { request_id: raw };
}

let sampleRequestSeq = 0;

/**
 * - `search`: apply the filter form and start from the first page.
 * - `reload`: first page again with the already-applied filters.
 * - `more`: next page with the applied filters and the server cursor.
 *
 * `reload` is a background refresh and doesn't toast on failure.
 */
async function loadSamples(
  mode: "search" | "reload" | "more",
  silent = mode === "reload",
): Promise<void> {
  if (mode === "more" && !sampleNextCursor.value) return;
  if (mode === "search") appliedSampleFilters = readSampleFilters();
  const cursor = mode === "more" ? sampleNextCursor.value : null;
  const generation = loadGeneration;
  const seq = ++sampleRequestSeq;
  sampleLoading.value = true;
  try {
    const page = await api.getRunSamples(
      id.value,
      { limit: 50, ...appliedSampleFilters, ...(cursor ?? {}) },
      { silent },
    );
    if (disposed || generation !== loadGeneration || seq !== sampleRequestSeq) return;
    samples.value = mode === "more" ? [...samples.value, ...page.items] : page.items;
    sampleNextCursor.value = page.next;
  } finally {
    if (seq === sampleRequestSeq) sampleLoading.value = false;
  }
}

function disposeChart(): void {
  chartResizeObserver?.disconnect();
  chartResizeObserver = null;
  chart?.dispose();
  chart = null;
}

function renderChart(points: TimePoint[]): void {
  if (!chartEl.value) return;
  if (chart && chart.getDom() !== chartEl.value) {
    disposeChart();
  }
  if (!chart) {
    chart = initChart(chartEl.value);
    if (typeof ResizeObserver !== "undefined") {
      chartResizeObserver = new ResizeObserver(() => {
        chart?.resize();
      });
      chartResizeObserver.observe(chartEl.value);
    }
  }
  const isDark = effectiveTheme.value === "black";
  const axisColor = isDark ? "#a1a1aa" : "#71717a";
  const gridColor = isDark ? "rgba(255,255,255,0.08)" : "rgba(9,9,11,0.07)";
  const primaryLine = isDark ? "#fafafa" : "#18181b";
  const showDot = points.length <= 2;

  const labels = points.map((p) => {
    const raw = p.unix_sec ?? p.started_at;
    if (!raw) return "";
    const ms = raw > 1e11 ? raw : raw * 1000;
    return new Date(ms).toLocaleTimeString("zh-CN", { hour12: false });
  });

  chart.setOption({
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      backgroundColor: isDark ? "#111114" : "#ffffff",
      borderColor: isDark ? "#27272a" : "#e4e4e7",
      textStyle: {
        color: isDark ? "#fafafa" : "#09090b",
        fontSize: 12,
      },
    },
    grid: { left: 44, right: 20, top: 34, bottom: 26 },
    legend: {
      data: ["总样本/s", "成功/s", "失败/s"],
      textStyle: { color: axisColor, fontSize: 12 },
    },
    xAxis: {
      type: "category",
      data: labels,
      axisLine: { lineStyle: { color: gridColor } },
      axisLabel: { color: axisColor, fontSize: 11 },
    },
    yAxis: {
      type: "value",
      splitLine: { lineStyle: { color: gridColor } },
      axisLabel: { color: axisColor, fontSize: 11 },
    },
    series: [
      {
        name: "总样本/s",
        type: "line",
        smooth: false,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2, type: "dashed" },
        areaStyle: {
          color: isDark ? "rgba(250,250,250,0.05)" : "rgba(24,24,27,0.04)",
        },
        data: points.map((p) => p.samples || 0),
        itemStyle: { color: primaryLine },
      },
      {
        name: "成功/s",
        type: "line",
        smooth: false,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2 },
        data: points.map((p) => p.success || 0),
        itemStyle: { color: isDark ? "#34d399" : "#059669" },
      },
      {
        name: "失败/s",
        type: "line",
        smooth: false,
        showSymbol: showDot,
        symbolSize: 6,
        lineStyle: { width: 2 },
        data: points.map((p) => p.failure || 0),
        itemStyle: { color: isDark ? "#fb7185" : "#e11d48" },
      },
    ],
  });
}

watch(effectiveTheme, () => {
  renderChart(lastSeries.value);
});

watch(isPageVisible, (visible) => {
  if (visible && isRunning.value) {
    void refreshAll().catch(() => {});
  }
});

function stopLiveStream(): void {
  sse?.close();
  sse = null;
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
}

function startLiveStream(): void {
  stopLiveStream();
  if (disposed || !id.value) return;
  try {
    const source = new EventSource(`/api/v1/runs/${id.value}/events`);
    sse = source;
    source.addEventListener("stats", (ev) => {
      if (isPageVisible.value) {
        applyStatsTick(String((ev as Event & { data?: unknown }).data ?? ""));
      }
    });
    source.onerror = () => {
      // The browser reconnects on its own unless the stream is closed for good
      // (e.g. HTTP error); only then fall back to polling.
      if (source.readyState !== EventSource.CLOSED || sse !== source) return;
      sse = null;
      startFallbackPoll();
    };
  } catch {
    startFallbackPoll();
  }
}

function startFallbackPoll(): void {
  if (pollTimer !== null || disposed) return;
  pollTimer = window.setInterval(() => {
    if (isPageVisible.value) {
      void refreshAll().catch(() => {});
    }
  }, FALLBACK_POLL_MS);
}

/** Load everything for the current run id and start live updates if needed. */
async function loadRun(): Promise<void> {
  if (!id.value) return;
  const generation = ++loadGeneration;
  stopLiveStream();
  disposeChart();
  initializedInputs = false;
  run.value = null;
  stats.value = null;
  windows.value = [];
  timeline.value = [];
  samples.value = [];
  sampleNextCursor.value = null;
  expandedSamples.value = new Set();
  breakdown.value = null;
  lastSeries.value = [];
  loadError.value = null;
  refreshError.value = null;
  try {
    await Promise.all([refreshAll(), loadSamples("search", true)]);
  } catch (err) {
    if (disposed || generation !== loadGeneration) return;
    loadError.value = err instanceof Error ? err.message : String(err);
  }
}

watch(id, () => {
  if (route.name !== "run-detail" || !route.params.id) return;
  void loadRun();
});

function retryRefresh(): void {
  void refreshAll().catch(() => {});
  void loadSamples("reload").catch(() => {});
}

async function handleStop(): Promise<void> {
  if (actionBusy.value) return;
  const confirmed = await askConfirm({
    title: "停止压测任务",
    description: `确定向任务「${run.value?.title || id.value}」发送停止信号吗？在途请求排空后任务将进入可续跑状态。`,
    confirmText: "确认停止",
    variant: "destructive",
  });
  if (!confirmed) return;
  actionBusy.value = "stop";
  try {
    await api.stopRun(id.value);
    toast.info("已发送停止信号");
    await refreshAll().catch(() => {});
    await loadSamples("reload").catch(() => {});
  } finally {
    actionBusy.value = null;
  }
}

async function handleResume(): Promise<void> {
  if (actionBusy.value) return;
  actionBusy.value = "resume";
  try {
    await api.resumeRun(id.value);
    toast.success("已恢复运行");
    // refreshAll sees the live status and reopens the stream.
    await refreshAll().catch(() => {});
  } finally {
    actionBusy.value = null;
  }
}

async function applyTrafficPatch(): Promise<void> {
  if (actionBusy.value) return;
  actionBusy.value = "tune";
  try {
    await api.patchTraffic(id.value, {
      qps: tuneQps.value,
      concurrency: tuneConcurrency.value,
    });
    toast.success("流量参数已实时下发");
    await refreshAll().catch(() => {});
  } finally {
    actionBusy.value = null;
  }
}

async function downloadReport(kind: "json" | "html"): Promise<void> {
  if (exportingKind.value) return;
  exportingKind.value = kind;
  try {
    const blob = await api.downloadReportBlob(id.value, kind);
    downloadBlob(blob, `run-${id.value}-report.${kind}`);
    toast.success(`已导出 ${kind.toUpperCase()} 报告`);
  } finally {
    exportingKind.value = null;
  }
}

function handleResize(): void {
  chart?.resize();
}

onMounted(async () => {
  window.addEventListener("resize", handleResize);
  await loadRun();
});

onBeforeUnmount(() => {
  disposed = true;
  window.removeEventListener("resize", handleResize);
  stopLiveStream();
  disposeChart();
});
</script>
