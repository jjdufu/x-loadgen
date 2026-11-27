<template>
  <div class="min-h-0 w-full flex-1 overflow-y-auto bg-background">
    <div class="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 lg:p-8">
      <div
        v-if="loadError && run"
        role="alert"
        class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-300"
      >
        <span>报告数据加载失败：{{ loadError }}</span>
        <UiButton variant="outline" size="sm" @click="loadRun">重试</UiButton>
      </div>
      <div
        v-else-if="refreshError && run"
        role="status"
        class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-300 bg-amber-50 px-4 py-2.5 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300"
      >
        <span class="min-w-0 break-words">
          刷新失败{{ isRunning ? "（自动重试中）" : "" }}：{{ refreshError }}
        </span>
        <UiButton variant="outline" size="sm" @click="retryRefresh">重试</UiButton>
      </div>

      <!-- Top Header & Controls -->
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-0 space-y-1.5">
          <div class="flex flex-wrap items-center gap-2">
            <UiButton
              variant="outline"
              size="icon-sm"
              aria-label="返回压测报告"
              @click="router.push({ name: 'runs' })"
            >
              <ArrowLeft class="h-4 w-4" />
            </UiButton>
            <h1 class="min-w-0 break-words text-lg font-semibold tracking-tight text-foreground sm:text-xl">
              {{ run?.title || "压测报告" }}
            </h1>
            <UiBadge v-if="run" :variant="runStatusBadgeVariant(run.status)">
              {{ formatRunStatus(run.status) }}
            </UiBadge>
            <UiBadge v-if="shapeLabel" variant="outline" class="font-mono text-[11px]">
              {{ shapeLabel }}
            </UiBadge>
            <UiBadge
              v-if="apdex?.score != null"
              :variant="apdex.score >= 0.85 ? 'success' : apdex.score >= 0.7 ? 'warning' : 'destructive'"
            >
              Apdex {{ apdex.score.toFixed(3) }}
            </UiBadge>
            <UiBadge
              v-if="run?.verdict && run.verdict.result !== 'none'"
              :variant="verdictVariant(run.verdict.result)"
            >
              SLA {{ formatVerdictResult(run.verdict.result) }}
            </UiBadge>
          </div>
          <p class="flex flex-wrap gap-x-3 gap-y-0.5 font-mono text-xs text-muted-foreground tabular-nums sm:pl-10">
            <span class="break-all">ID {{ id }}</span>
            <template v-if="run">
              <span>{{ formatRunMode(run.mode) }}</span>
              <span>{{ run.processes ?? 1 }} 进程</span>
              <span v-if="run.environment_name">环境 {{ run.environment_name }}</span>
              <span v-if="summary?.effective_elapsed_ms != null">
                有效时长 {{ formatDurationMs(summary.effective_elapsed_ms) }}
              </span>
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
            variant="default"
            size="sm"
            :loading="replaying"
            @click="handleRerun"
          >
            <RotateCcw v-if="!replaying" class="h-3.5 w-3.5" />
            <span>重跑</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            @click="router.push({ name: 'run-compare', query: { base: id } })"
          >
            <GitCompare class="h-3.5 w-3.5" />
            <span>对比</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            :disabled="exportingKind !== null"
            @click="downloadReport('json')"
          >
            <Download class="h-3.5 w-3.5" />
            <span>{{ exportingKind === "json" ? "导出中…" : "JSON" }}</span>
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            :disabled="exportingKind !== null"
            @click="downloadReport('html')"
          >
            <FileCode2 class="h-3.5 w-3.5" />
            <span>{{ exportingKind === "html" ? "导出中…" : "HTML" }}</span>
          </UiButton>
        </div>
      </div>

      <UiCard
        v-if="loadError && !run"
        role="alert"
        class="space-y-4 p-8 text-center"
      >
        <h2 class="text-base font-semibold text-foreground">加载失败</h2>
        <p class="mx-auto max-w-md break-words text-sm text-muted-foreground">{{ loadError }}</p>
        <div class="flex justify-center gap-2.5">
          <UiButton variant="outline" size="sm" @click="router.push({ name: 'runs' })">
            返回列表
          </UiButton>
          <UiButton variant="default" size="sm" @click="loadRun">重试</UiButton>
        </div>
      </UiCard>

      <UiCard v-else-if="!run" class="p-10 text-center text-sm text-muted-foreground">
        加载中…
      </UiCard>

      <template v-else>
        <!-- Error & Data Integrity Banners -->
        <div
          v-if="run.error"
          class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-4 py-3 font-mono text-xs text-rose-700 dark:text-rose-400"
        >
          任务错误：{{ run.error }}
        </div>
        <div
          v-if="integrityWarnings.length"
          class="rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2.5 font-mono text-xs text-amber-800 dark:text-amber-300"
        >
          数据完整性提示：{{ integrityWarnings.join("；") }}
        </div>

        <!-- 12-Card Executive KPI Grid -->
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
          <UiCard
            v-for="card in metricCards"
            :key="card.label"
            class="min-w-0 space-y-1 p-3 sm:p-3.5"
          >
            <div class="truncate text-xs font-medium text-muted-foreground">
              {{ card.label }}
            </div>
            <div
              class="truncate font-mono text-lg font-semibold tracking-tight tabular-nums"
              :class="card.colorClass ?? 'text-foreground'"
              :title="String(card.value)"
            >
              {{ card.value }}
            </div>
            <div v-if="card.sub" class="truncate font-mono text-[11px] text-muted-foreground tabular-nums">
              {{ card.sub }}
            </div>
          </UiCard>
        </div>

        <!-- Live Traffic Tuner Card -->
        <UiCard v-if="isRunning" class="p-4">
          <form class="flex flex-wrap items-end gap-4" @submit.prevent="applyTrafficPatch">
            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                目标 QPS
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
                并发上限
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
            <UiButton type="submit" variant="default" :disabled="actionBusy !== null">
              {{ actionBusy === "tune" ? "下发中…" : "应用" }}
            </UiButton>
          </form>
        </UiCard>

        <!-- Full Charts Matrix + Adaptive Protocol Breakdown -->
        <ReportChartsGrid
          :series="lastSeries"
          :breakdown="breakdown"
          :timeline="timeline"
          :windows="windows"
          :apdex="apdex"
        />

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
              class="relative inline-flex h-full cursor-pointer items-center gap-1.5 rounded-t-md px-3 text-xs font-medium whitespace-nowrap transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              :class="
                activeDetailTab === tab.id
                  ? 'text-foreground hover:bg-hover-bg'
                  : 'text-muted-foreground hover:bg-hover-bg hover:text-foreground active:bg-active-bg'
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

          <!-- 1. Breakdown -->
          <div v-if="activeDetailTab === 'breakdown'" class="space-y-5 p-5 text-xs">
            <div v-if="!breakdown" class="py-6 text-center text-sm text-muted-foreground">
              暂无分项统计
            </div>
            <template v-else>
              <div
                v-for="section in breakdownSections"
                :key="section.title"
                class="overflow-x-auto"
              >
                <div class="mb-2 font-medium text-foreground">{{ section.title }}</div>
                <table class="w-full border-collapse text-left font-mono tabular-nums">
                  <thead>
                    <tr class="border-b border-border text-muted-foreground">
                      <th class="py-2 pr-3 font-medium">{{ section.label }}</th>
                      <th class="py-2 pr-3 font-medium">样本</th>
                      <th class="py-2 pr-3 font-medium">占比</th>
                      <th class="py-2 pr-3 font-medium">QPS</th>
                      <th class="py-2 pr-3 font-medium">错误率</th>
                      <th class="py-2 pr-3 font-medium">Avg</th>
                      <th class="py-2 pr-3 font-medium">P50</th>
                      <th class="py-2 pr-3 font-medium">P90</th>
                      <th class="py-2 pr-3 font-medium">P95</th>
                      <th class="py-2 pr-3 font-medium">P99</th>
                      <th class="py-2 pr-3 font-medium">Max</th>
                      <th class="py-2 font-medium">均体</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    <tr v-for="g in section.rows" :key="g.product_id ?? g.tenant">
                      <td class="py-2 pr-3 text-foreground">{{ g.product_id ?? g.tenant }}</td>
                      <td class="py-2 pr-3">{{ g.samples }}</td>
                      <td class="py-2 pr-3 text-muted-foreground">{{ g.share_pct != null ? `${g.share_pct}%` : "-" }}</td>
                      <td class="py-2 pr-3">{{ g.qps != null ? g.qps.toFixed(1) : "-" }}</td>
                      <td
                        class="py-2 pr-3"
                        :class="(g.error_rate ?? 0) > 0 ? 'text-rose-600 dark:text-rose-400' : 'text-emerald-600 dark:text-emerald-400'"
                      >
                        {{ g.error_rate != null ? `${(g.error_rate * 100).toFixed(2)}%` : "0.00%" }}
                      </td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.avg_us) }}</td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.p50_us) }}</td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.p90_us) }}</td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.p95_us) }}</td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.p99_us) }}</td>
                      <td class="py-2 pr-3">{{ formatLatencyUs(g.max_us) }}</td>
                      <td class="py-2 text-muted-foreground">{{ formatBytes(g.avg_bytes) }}</td>
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
                <div class="mb-2 font-medium text-foreground">
                  错误 Top {{ breakdown.top_errors.length }}
                </div>
                <div v-if="breakdown.top_errors.length === 0" class="text-muted-foreground">
                  无失败样本
                </div>
                <table v-else class="w-full border-collapse text-left font-mono">
                  <thead>
                    <tr class="border-b border-border text-muted-foreground">
                      <th class="py-2 pr-4 font-medium">分类</th>
                      <th class="py-2 pr-4 font-medium">错误</th>
                      <th class="py-2 pr-4 font-medium">次数</th>
                      <th class="py-2 pr-4 font-medium">占失败比</th>
                      <th class="py-2 font-medium">状态码</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    <tr v-for="e in breakdown.top_errors" :key="`${e.error_class}|${e.error}`">
                      <td class="py-2 pr-4 text-foreground">{{ e.error_class }}</td>
                      <td class="max-w-xl break-all py-2 pr-4 text-rose-600 dark:text-rose-400">{{ e.error }}</td>
                      <td class="py-2 pr-4 tabular-nums">{{ e.count }}</td>
                      <td class="py-2 pr-4 tabular-nums text-muted-foreground">
                        {{ e.share_pct != null ? `${e.share_pct}%` : "-" }}
                      </td>
                      <td class="py-2 tabular-nums">
                        {{ Object.entries(e.http_status).map(([c, n]) => `${c}×${n}`).join(" ") }}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
          </div>

          <!-- 2. Samples Explorer -->
          <div v-else-if="activeDetailTab === 'samples'" class="space-y-4 p-4 sm:p-5">
            <div class="flex flex-wrap items-end gap-3 border-b border-border pb-4">
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  结果状态
                </label>
                <UiSelect v-model="sampleFilterSuccess" size="sm" wrapper-class="w-32" aria-label="结果状态">
                  <option value="all">全部</option>
                  <option value="false">仅失败</option>
                  <option value="true">仅成功</option>
                </UiSelect>
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
                  错误分类
                </label>
                <UiSelect v-model="sampleFilterErrClass" size="sm" wrapper-class="w-40" aria-label="错误分类">
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
                  aria-label="最低延迟"
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
                  placeholder="tenant-a"
                  aria-label="租户"
                  class="w-32"
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
                  placeholder="request_id / trace_id"
                  aria-label="请求号或链路号"
                  class="w-64"
                />
              </div>
              <UiButton variant="outline" size="sm" :disabled="sampleLoading" @click="loadSamples('search')">
                检索
              </UiButton>
            </div>

            <div v-if="samples.length === 0" class="py-10 text-center text-sm text-muted-foreground">
              暂无匹配样本
            </div>

            <div v-else class="overflow-x-auto">
              <table class="w-full border-collapse whitespace-nowrap text-left font-mono text-xs">
                <thead>
                  <tr class="border-b border-border text-muted-foreground">
                    <th class="py-2.5 pr-3 font-medium">进程#序号</th>
                    <th class="py-2.5 pr-3 font-medium">开始时间</th>
                    <th class="py-2.5 pr-3 font-medium">接口</th>
                    <th class="py-2.5 pr-3 font-medium">状态</th>
                    <th class="py-2.5 pr-3 font-medium">总耗时</th>
                    <th class="py-2.5 pr-3 font-medium">TTFB / 传输</th>
                    <th class="py-2.5 pr-3 font-medium">字节</th>
                    <th class="py-2.5 pr-3 font-medium">请求号 / 链路号</th>
                    <th class="py-2.5 font-medium">诊断</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border">
                  <template v-for="s in samples" :key="`${s.worker}-${s.seq}`">
                    <tr
                      class="transition-colors hover:bg-accent-soft-hover"
                      :class="s.response_body ? 'cursor-pointer' : ''"
                      @click="toggleSampleBody(s)"
                    >
                      <td class="py-2 pr-3 text-muted-foreground tabular-nums">w{{ s.worker }}#{{ s.seq }}</td>
                      <td class="py-2 pr-3 text-muted-foreground tabular-nums">{{ formatSampleTime(s.start_unix_ms) }}</td>
                      <td class="py-2 pr-3 text-foreground">{{ s.product_name || s.product_id }}</td>
                      <td class="py-2 pr-3">
                        <span
                          class="font-semibold"
                          :class="s.success ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400'"
                        >
                          {{ s.http_status ?? (s.success ? "成功" : "失败") }}
                        </span>
                      </td>
                      <td class="py-2 pr-3 text-foreground tabular-nums">{{ formatLatencyUs(s.latency_us) }}</td>
                      <td class="py-2 pr-3 text-muted-foreground tabular-nums">
                        {{ formatLatencyUs(s.ttfb_us) }} / {{ formatLatencyUs(s.transfer_us) }}
                        <span v-if="s.crypto_seal_us != null" class="ml-1 text-violet-600 dark:text-violet-400">
                          (密 {{ formatLatencyUs((s.crypto_seal_us ?? 0) + (s.crypto_unseal_us ?? 0)) }})
                        </span>
                      </td>
                      <td class="py-2 pr-3 text-muted-foreground tabular-nums">{{ formatBytes(s.response_bytes) }}</td>
                      <td class="py-2 pr-3 text-[11px] leading-tight text-muted-foreground">
                        <div class="select-all">{{ s.request_id || "-" }}</div>
                        <div class="select-all">{{ s.trace_id || "-" }}</div>
                      </td>
                      <td class="max-w-sm truncate py-2 text-rose-600 dark:text-rose-400">
                        <span v-if="s.response_body" class="mr-1 text-muted-foreground">
                          {{ expandedSamples.has(sampleKey(s)) ? "▾" : "▸" }}
                        </span>
                        {{ s.error_class ? `[${s.error_class}] ` : "" }}{{ s.error ?? "" }}
                      </td>
                    </tr>
                    <tr v-if="s.response_body && expandedSamples.has(sampleKey(s))">
                      <td colspan="9" class="bg-muted/30 px-3 py-2">
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

          <!-- 3. Structured SLA Verdict Table -->
          <div v-else-if="activeDetailTab === 'verdict'" class="space-y-4 p-5 text-sm">
            <div
              v-if="!run.verdict || run.verdict.result === 'none'"
              class="py-6 text-muted-foreground"
            >
              未配置 SLA 门槛规则
            </div>
            <template v-else>
              <div class="flex items-center gap-2 font-medium text-foreground">
                <span>综合结论</span>
                <UiBadge :variant="verdictVariant(run.verdict.result)">
                  {{ formatVerdictResult(run.verdict.result) }}
                </UiBadge>
              </div>
              <div class="overflow-x-auto">
                <table class="w-full border-collapse text-left font-mono text-xs tabular-nums">
                  <thead>
                    <tr class="border-b border-border text-muted-foreground">
                      <th class="py-2 pr-4 font-medium">规则项</th>
                      <th class="py-2 pr-4 font-medium">目标阈值</th>
                      <th class="py-2 pr-4 font-medium">实测值</th>
                      <th class="py-2 font-medium">状态</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    <tr v-for="row in slaRows" :key="row.key">
                      <td class="py-2.5 pr-4 font-sans font-medium text-foreground">{{ row.label }}</td>
                      <td class="py-2.5 pr-4 text-muted-foreground">{{ row.wanted }}</td>
                      <td class="py-2.5 pr-4 text-foreground">{{ row.actual }}</td>
                      <td class="py-2.5">
                        <UiBadge :variant="row.passed ? 'success' : 'destructive'">
                          {{ row.passed ? "通过" : "未达标" }}
                        </UiBadge>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
          </div>

          <!-- 4. Loop Windows -->
          <div v-else-if="activeDetailTab === 'windows'" class="overflow-x-auto p-5">
            <div v-if="windows.length === 0" class="py-6 text-center text-sm text-muted-foreground">
              无循环窗口记录
            </div>
            <table v-else class="w-full border-collapse text-left font-mono text-xs tabular-nums">
              <thead>
                <tr class="border-b border-border text-muted-foreground">
                  <th class="py-2.5 pr-4 font-medium">轮次</th>
                  <th class="py-2.5 pr-4 font-medium">样本</th>
                  <th class="py-2.5 pr-4 font-medium">成功</th>
                  <th class="py-2.5 pr-4 font-medium">失败</th>
                  <th class="py-2.5 pr-4 font-medium">P50</th>
                  <th class="py-2.5 pr-4 font-medium">P95</th>
                  <th class="py-2.5 font-medium">P99</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border">
                <tr v-for="w in windows" :key="w.window_index">
                  <td class="py-2 pr-4 text-foreground">#{{ w.window_index }}</td>
                  <td class="py-2 pr-4 text-foreground">{{ w.samples }}</td>
                  <td class="py-2 pr-4 text-emerald-600 dark:text-emerald-400">{{ w.success }}</td>
                  <td class="py-2 pr-4 text-rose-600 dark:text-rose-400">{{ w.failure }}</td>
                  <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p50_us) }}</td>
                  <td class="py-2 pr-4 text-foreground">{{ formatLatencyUs(w.p95_us) }}</td>
                  <td class="py-2 text-foreground">{{ formatLatencyUs(w.p99_us) }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- 5. Lifecycle Timeline -->
          <div v-else-if="activeDetailTab === 'timeline'" class="divide-y divide-border p-5 text-xs">
            <div v-if="formattedTimeline.length === 0" class="py-6 text-center text-sm text-muted-foreground">
              暂无时间线事件
            </div>
            <div
              v-for="ev in formattedTimeline"
              :key="ev.id"
              class="flex flex-wrap items-center justify-between gap-2 py-2.5"
            >
              <div class="min-w-0">
                <span class="mr-3 font-medium text-foreground">{{ ev.label }}</span>
                <span v-if="ev.detail" class="text-muted-foreground">{{ ev.detail }}</span>
              </div>
              <span class="font-mono text-muted-foreground tabular-nums">{{ ev.time }}</span>
            </div>
          </div>

          <!-- 6. Redacted Snapshot -->
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
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  ArrowLeft,
  Download,
  FileCode2,
  GitCompare,
  Play,
  RotateCcw,
  Square,
} from "lucide-vue-next";
import { api } from "../api/workspace";
import type {
  ReportBreakdown,
  ReportSummary,
  RunStats,
  RunView,
  SampleCursor,
  SampleFilters,
  SampleRow,
  TimePoint,
  TimelineEvent,
  WindowRow,
} from "../api/types";
import CodeMirrorEditor from "../components/common/CodeMirrorEditor.vue";
import ReportChartsGrid from "../components/report/ReportChartsGrid.vue";
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
import { toast } from "../modules/toast";
import { openRunReplayInWorkspace } from "../stores/workspace";

const route = useRoute();
const router = useRouter();
const id = computed(() => String(route.params.id ?? ""));

const run = ref<RunView | null>(null);
const stats = ref<RunStats | null>(null);
const summary = ref<ReportSummary | null>(null);
const breakdown = ref<ReportBreakdown | null>(null);
const windows = ref<WindowRow[]>([]);
const timeline = ref<TimelineEvent[]>([]);
const samples = ref<SampleRow[]>([]);
const lastSeries = ref<TimePoint[]>([]);
const sampleNextCursor = ref<SampleCursor | null>(null);
let appliedSampleFilters: SampleFilters = {};
const sampleLoading = ref(false);
const actionBusy = ref<"stop" | "resume" | "tune" | null>(null);
const exportingKind = ref<"json" | "html" | null>(null);
const replaying = ref(false);

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
const sampleFilterId = ref("");
const expandedSamples = ref(new Set<string>());

const tuneQps = ref(20);
const tuneConcurrency = ref(10);
const activeDetailTab = ref<
  "breakdown" | "samples" | "verdict" | "windows" | "timeline" | "snapshot"
>("breakdown");

let sse: EventSource | null = null;
let pollTimer: number | null = null;
let initializedInputs = false;
let disposed = false;
let loadGeneration = 0;
let refreshInFlight: Promise<void> | null = null;
let refreshQueued = false;
let lastFullRefreshAt = 0;
const FULL_REFRESH_INTERVAL_MS = 5000;
const FALLBACK_POLL_MS = 2000;
const loadError = ref<string | null>(null);
const refreshError = ref<string | null>(null);
const SILENT = { silent: true } as const;

function isLiveStatus(status: string | undefined): boolean {
  return status === "running" || status === "stopping";
}

const isRunning = computed(() => isLiveStatus(run.value?.status));
const canResume = computed(
  () => run.value?.status === "stopped" || run.value?.status === "interrupted",
);
const snapshotJson = computed(() => JSON.stringify(run.value?.snapshot ?? {}, null, 2));

const shapeLabel = computed(() => {
  const s = summary.value?.shape;
  if (s === "single_http") return "单接口 HTTP";
  if (s === "multi_http") return "多接口 HTTP";
  if (s === "single_mazbri") return "单接口 Mazbri";
  if (s === "multi_mazbri") return "多接口 Mazbri";
  if (s === "mixed") return "混合协议";
  return "";
});

const apdex = computed(() => summary.value?.apdex ?? breakdown.value?.apdex ?? null);

const integrityWarnings = computed(() => {
  const out: string[] = [];
  const d = summary.value?.data_integrity;
  if ((d?.missed_sends ?? stats.value?.missed_sends ?? 0) > 0) {
    out.push(`开环过载漏发 ${d?.missed_sends ?? stats.value?.missed_sends} 次`);
  }
  if ((d?.dropped_details ?? stats.value?.dropped_details ?? 0) > 0) {
    out.push(`明细队列丢弃 ${d?.dropped_details ?? stats.value?.dropped_details} 条`);
  }
  return out;
});

const breakdownSections = computed(() => {
  if (!breakdown.value) return [];
  const secs = [{ title: "按接口 / 产品", label: "接口", rows: breakdown.value.products }];
  const realTenants = breakdown.value.tenants.filter((t) => t.tenant !== "(none)");
  if (realTenants.length > 0) {
    secs.push({ title: "按租户", label: "租户", rows: realTenants });
  }
  return secs;
});

const slaRows = computed(() => {
  const v = run.value?.verdict;
  if (!v?.assert) return [];
  const failedSet = new Set((v.failed ?? []).map((f) => f.check));
  const a = v.actual;
  const defs: { key: string; label: string; want: number | null | undefined; got: number | null | undefined; op: string; unit: string }[] = [
    { key: "p50_ms", label: "P50 延迟", want: v.assert.p50_ms, got: a?.p50_ms, op: "≤", unit: " ms" },
    { key: "p95_ms", label: "P95 延迟", want: v.assert.p95_ms, got: a?.p95_ms, op: "≤", unit: " ms" },
    { key: "p99_ms", label: "P99 延迟", want: v.assert.p99_ms, got: a?.p99_ms, op: "≤", unit: " ms" },
    { key: "max_ms", label: "最大延迟", want: v.assert.max_ms, got: a?.max_ms, op: "≤", unit: " ms" },
    {
      key: "max_error_rate",
      label: "错误率上限",
      want: v.assert.max_error_rate != null ? v.assert.max_error_rate * 100 : null,
      got: a?.error_rate != null ? a.error_rate * 100 : null,
      op: "≤",
      unit: "%",
    },
    {
      key: "min_qps_ratio",
      label: "QPS 达成率下限",
      want: v.assert.min_qps_ratio != null ? v.assert.min_qps_ratio * 100 : null,
      got: a?.qps_ratio != null ? a.qps_ratio * 100 : null,
      op: "≥",
      unit: "%",
    },
  ];
  return defs
    .filter((d) => d.want != null)
    .map((d) => ({
      key: d.key,
      label: d.label,
      wanted: `${d.op} ${Number(d.want).toFixed(2)}${d.unit}`,
      actual: d.got != null ? `${Number(d.got).toFixed(2)}${d.unit}` : "-",
      passed: !failedSet.has(d.key),
    }));
});

const formattedTimeline = computed(() =>
  timeline.value.map((ev) => {
    const formatted = formatTimelineEvent(ev.event, ev.payload);
    return { id: ev.id, label: formatted.label, detail: formatted.detail, time: formatTimeMs(ev.ts) };
  }),
);

const detailTabs = computed(() => [
  { id: "breakdown" as const, label: "分项统计", count: breakdown.value?.products.length ?? 0 },
  { id: "samples" as const, label: "样本明细", count: samples.value.length },
  { id: "verdict" as const, label: "SLA 判定", count: slaRows.value.length },
  { id: "windows" as const, label: "循环窗口", count: windows.value.length },
  { id: "timeline" as const, label: "事件时间线", count: timeline.value.length },
  { id: "snapshot" as const, label: "执行快照", count: 0 },
]);

const metricCards = computed(() => {
  const s = stats.value;
  const sm = summary.value;
  const total = s?.samples ?? 0;
  const ok = s?.success ?? 0;
  const fail = s?.failure ?? 0;
  const okRate = sm?.success_rate ?? (total > 0 ? ok / total : null);
  const errRate = sm?.error_rate ?? (total > 0 ? fail / total : null);
  const avgQps = sm?.achieved_qps ?? run.value?.verdict?.actual?.achieved_qps ?? null;
  const peakQps = sm?.peak_qps ?? (lastSeries.value.length ? Math.max(...lastSeries.value.map((p) => p.samples || 0)) : null);
  const ratioSub = sm?.qps_ratio != null ? `达成率 ${(sm.qps_ratio * 100).toFixed(1)}%` : undefined;
  return [
    { label: "样本总数", value: total, sub: `成功 ${ok} · 失败 ${fail}` },
    {
      label: "平均 QPS",
      value: avgQps != null ? `${avgQps.toFixed(1)}` : "-",
      sub: ratioSub ?? (peakQps != null ? `峰值 ${peakQps} req/s` : undefined),
    },
    {
      label: "峰值 QPS",
      value: peakQps != null ? `${peakQps}` : "-",
      sub: `峰值并发 ${sm?.peak_in_flight ?? s?.peak_in_flight ?? 0}`,
    },
    {
      label: "成功率",
      value: okRate != null ? `${(okRate * 100).toFixed(2)}%` : "-",
      sub: `错误率 ${errRate != null ? `${(errRate * 100).toFixed(2)}%` : "-"}`,
      colorClass: fail > 0 ? "text-rose-600 dark:text-rose-400" : "text-emerald-600 dark:text-emerald-400",
    },
    {
      label: "Apdex",
      value: apdex.value?.score != null ? apdex.value.score.toFixed(3) : "-",
      sub: apdex.value ? `T=${apdex.value.satisfied_ms}ms` : undefined,
    },
    {
      label: "平均带宽",
      value: sm?.avg_bandwidth_bps != null ? `${formatBytes(sm.avg_bandwidth_bps)}/s` : formatBytes(s?.bytes),
      sub: `累计 ${formatBytes(sm?.total_response_bytes ?? s?.bytes)}`,
    },
    { label: "平均延迟", value: formatLatencyUs(s?.avg_us), sub: `标准差 ${formatLatencyUs(s?.stdev_us)}` },
    { label: "P50 延迟", value: formatLatencyUs(s?.p50_us), sub: `P75 ${formatLatencyUs(s?.p75_us)}` },
    { label: "P90 延迟", value: formatLatencyUs(s?.p90_us), sub: undefined },
    { label: "P95 延迟", value: formatLatencyUs(s?.p95_us), sub: undefined },
    { label: "P99 延迟", value: formatLatencyUs(s?.p99_us), sub: `P99.9 ${formatLatencyUs(s?.p999_us)}` },
    { label: "最大延迟", value: formatLatencyUs(s?.max_us), sub: `最小 ${formatLatencyUs(s?.min_us)}` },
  ];
});

function formatDurationMs(ms: number): string {
  const secs = ms / 1000;
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const m = Math.floor(secs / 60);
  return `${m}m ${(secs % 60).toFixed(0)}s`;
}

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

function verdictVariant(result: string): "success" | "destructive" | "warning" {
  if (result === "pass") return "success";
  if (result === "fail") return "destructive";
  return "warning";
}

async function handleRerun(): Promise<void> {
  if (replaying.value) return;
  replaying.value = true;
  try {
    const replay = await api.getRunReplay(id.value);
    openRunReplayInWorkspace(replay);
    await router.push({ name: "workspace" });
    toast.success("已载入历史配置");
  } finally {
    replaying.value = false;
  }
}

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
  run.value = runData;
  stats.value = statsData;
  windows.value = winData;
  timeline.value = timeData;

  let effectiveSeries = seriesData;
  if (!isLiveStatus(runData.status) || !summary.value) {
    try {
      const report = await api.getRunReport(id.value, SILENT);
      if (disposed || generation !== loadGeneration) return;
      if (report.timeseries?.length) effectiveSeries = report.timeseries;
      summary.value = report.summary ?? null;
      breakdown.value = report.breakdown ?? null;
      if (stats.value && report.percentiles) {
        stats.value = {
          ...stats.value,
          min_us: report.percentiles.min_us ?? stats.value.min_us,
          avg_us: report.percentiles.avg_us ?? stats.value.avg_us,
          stdev_us: report.percentiles.stdev_us ?? stats.value.stdev_us,
          p75_us: report.percentiles.p75_us ?? stats.value.p75_us,
          p90_us: report.percentiles.p90_us ?? stats.value.p90_us,
          p999_us: report.percentiles.p999_us ?? stats.value.p999_us,
        };
      }
    } catch {
      // Keep live stats if report is still finalizing
    }
  }
  lastSeries.value = effectiveSeries;

  if (!initializedInputs) {
    if (statsData.current_qps) tuneQps.value = statsData.current_qps;
    if (statsData.current_concurrency) tuneConcurrency.value = statsData.current_concurrency;
    initializedInputs = true;
  }
  syncLiveStream(wasLive);
}

function syncLiveStream(wasLive: boolean): void {
  if (disposed) return;
  const live = isLiveStatus(run.value?.status);
  if (live && !sse && pollTimer === null) {
    startLiveStream();
  } else if (!live && (sse || pollTimer !== null)) {
    stopLiveStream();
  }
  if (wasLive && !live) {
    void loadSamples("reload").catch(() => {});
  }
}

function applyStatsTick(raw: string): void {
  let payload: Partial<RunStats>;
  try {
    payload = JSON.parse(raw) as Partial<RunStats>;
  } catch {
    return;
  }
  if (payload.id && payload.id !== id.value) return;
  if (stats.value) stats.value = { ...stats.value, ...payload };
  const statusChanged = Boolean(payload.status && run.value && payload.status !== run.value.status);
  if (statusChanged || Date.now() - lastFullRefreshAt >= FULL_REFRESH_INTERVAL_MS) {
    void refreshAll().catch(() => {});
  }
}

function readSampleFilters(): SampleFilters {
  const minMs = Number(sampleFilterMinMs.value);
  return {
    success: sampleFilterSuccess.value === "all" ? undefined : sampleFilterSuccess.value === "true",
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
    if (isPageVisible.value) void refreshAll().catch(() => {});
  }, FALLBACK_POLL_MS);
}

async function loadRun(): Promise<void> {
  if (!id.value) return;
  const generation = ++loadGeneration;
  stopLiveStream();
  initializedInputs = false;
  run.value = null;
  stats.value = null;
  summary.value = null;
  breakdown.value = null;
  windows.value = [];
  timeline.value = [];
  samples.value = [];
  sampleNextCursor.value = null;
  expandedSamples.value = new Set();
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

watch(isPageVisible, (visible) => {
  if (visible && isRunning.value) void refreshAll().catch(() => {});
});

function retryRefresh(): void {
  void refreshAll().catch(() => {});
  void loadSamples("reload").catch(() => {});
}

async function handleStop(): Promise<void> {
  if (actionBusy.value) return;
  const confirmed = await askConfirm({
    title: "停止压测",
    description: "在途请求排空后任务将进入可续跑状态。",
    confirmText: "停止",
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
    await refreshAll().catch(() => {});
  } finally {
    actionBusy.value = null;
  }
}

async function applyTrafficPatch(): Promise<void> {
  if (actionBusy.value) return;
  actionBusy.value = "tune";
  try {
    await api.patchTraffic(id.value, { qps: tuneQps.value, concurrency: tuneConcurrency.value });
    toast.success("流量参数已下发");
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
    toast.success(`已导出 ${kind.toUpperCase()}`);
  } finally {
    exportingKind.value = null;
  }
}

onMounted(async () => {
  await loadRun();
});

onBeforeUnmount(() => {
  disposed = true;
  stopLiveStream();
});
</script>
