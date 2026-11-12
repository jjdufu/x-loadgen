<template>
  <div class="space-y-4">
    <!-- Core Model & Concurrency -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          发压模型 (Load Model)
        </label>
        <select
          :value="config.model"
          class="select"
          @change="setModel(($event.target as HTMLSelectElement).value)"
        >
          <option value="open_loop">开放环恒定速率 (open_loop / 固定 QPS)</option>
          <option value="closed_loop">闭合环并发饱和 (closed_loop / 仅限并发)</option>
        </select>
      </div>
      <div v-if="config.model === 'open_loop'">
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          目标速率 (QPS) *
        </label>
        <input
          :value="config.qps ?? 20"
          type="number"
          min="1"
          class="input font-mono"
          @input="patch({ qps: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1) })"
        />
      </div>
      <div>
        <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
          最大并发数 (Concurrency) *
        </label>
        <input
          :value="config.concurrency"
          type="number"
          min="1"
          class="input font-mono"
          @input="patch({ concurrency: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1) })"
        />
      </div>
    </div>

    <!-- Budget XOR: Duration vs Total Requests -->
    <div class="p-3 rounded border border-dividerLight bg-primaryLight/40 space-y-3">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-secondaryDark">单轮压测预算 (二选一互斥)</span>
        <div class="flex items-center space-x-1 bg-primary rounded border border-divider p-0.5">
          <button
            type="button"
            class="px-2.5 py-1 rounded text-tiny font-semibold transition"
            :class="
              budgetKind === 'duration'
                ? 'bg-accent text-accentContrast'
                : 'text-secondary hover:text-secondaryDark'
            "
            @click="switchBudget('duration')"
          >
            按持续时长 (秒)
          </button>
          <button
            type="button"
            class="px-2.5 py-1 rounded text-tiny font-semibold transition"
            :class="
              budgetKind === 'requests'
                ? 'bg-accent text-accentContrast'
                : 'text-secondary hover:text-secondaryDark'
            "
            @click="switchBudget('requests')"
          >
            按总请求数 (次)
          </button>
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
        <div v-if="budgetKind === 'duration'">
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            持续秒数 (duration_secs) *
          </label>
          <input
            :value="config.duration_secs ?? 10"
            type="number"
            min="1"
            class="input font-mono"
            @input="
              patch({
                duration_secs: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1),
                requests: null,
              })
            "
          />
        </div>
        <div v-else>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            请求总数 (requests) *
          </label>
          <input
            :value="config.requests ?? 100"
            type="number"
            min="1"
            class="input font-mono"
            @input="
              patch({
                requests: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1),
                duration_secs: null,
              })
            "
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            预热时长 (warmup_secs)
          </label>
          <input
            :value="config.warmup_secs"
            type="number"
            min="0"
            class="input font-mono"
            @input="patch({ warmup_secs: Math.max(0, Number(($event.target as HTMLInputElement).value) || 0) })"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            线性爬坡 (ramp_secs)
          </label>
          <input
            :value="config.ramp_secs"
            type="number"
            min="0"
            class="input font-mono"
            @input="patch({ ramp_secs: Math.max(0, Number(($event.target as HTMLInputElement).value) || 0) })"
          />
        </div>
      </div>
    </div>

    <!-- Advanced Multi-Process, Loop & Timeouts -->
    <details class="rounded border border-dividerLight bg-primaryLight/20">
      <summary class="px-3 py-2 cursor-pointer flex items-center justify-between font-semibold text-secondaryDark">
        <span>多进程集群、循环窗口与超时配置</span>
        <span class="text-tiny text-secondaryLight font-mono">
          processes={{ config.processes }} · timeout={{ config.timeout_secs }}s
        </span>
      </summary>
      <div class="p-3 border-t border-dividerLight space-y-3">
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              工作进程数 (1–64)
            </label>
            <input
              :value="config.processes"
              type="number"
              min="1"
              max="64"
              class="input font-mono"
              @input="patch({ processes: Math.min(64, Math.max(1, Number(($event.target as HTMLInputElement).value) || 1)) })"
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              QPS 分配模式
            </label>
            <select
              :value="config.qps_mode"
              class="select"
              @change="patch({ qps_mode: ($event.target as HTMLSelectElement).value })"
            >
              <option value="total">全局均摊 (total)</option>
              <option value="per_process">每进程独立 (per_process)</option>
            </select>
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              请求超时 (秒)
            </label>
            <input
              :value="config.timeout_secs"
              type="number"
              min="1"
              class="input font-mono"
              @input="patch({ timeout_secs: Math.max(1, Number(($event.target as HTMLInputElement).value) || 30) })"
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              连接超时 (秒)
            </label>
            <input
              :value="config.connect_timeout_secs"
              type="number"
              min="1"
              class="input font-mono"
              @input="patch({ connect_timeout_secs: Math.max(1, Number(($event.target as HTMLInputElement).value) || 5) })"
            />
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-6 pt-1">
          <HoppSmartToggle
            :on="config.loop_enabled"
            @change="patch({ loop_enabled: !config.loop_enabled })"
          >
            启用多轮循环压测 (Loop Mode)
          </HoppSmartToggle>
          <HoppSmartToggle
            :on="config.include_request_id"
            @change="patch({ include_request_id: !config.include_request_id })"
          >
            自动注入 X-Request-ID 追踪头
          </HoppSmartToggle>
        </div>

        <div v-if="config.loop_enabled" class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              循环轮数 (rounds，留空为无限直到手动停止)
            </label>
            <input
              :value="config.rounds ?? ''"
              type="number"
              min="1"
              placeholder="不限轮数"
              class="input font-mono"
              @input="
                patch({
                  rounds: ($event.target as HTMLInputElement).value
                    ? Math.max(1, Number(($event.target as HTMLInputElement).value))
                    : null,
                })
              "
            />
          </div>
          <div>
            <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
              轮间静默间隔秒数 (loop_interval_secs)
            </label>
            <input
              :value="config.loop_interval_secs"
              type="number"
              min="0"
              class="input font-mono"
              @input="patch({ loop_interval_secs: Math.max(0, Number(($event.target as HTMLInputElement).value) || 0) })"
            />
          </div>
        </div>
      </div>
    </details>

    <!-- SLA Assertions -->
    <details class="rounded border border-dividerLight bg-primaryLight/20">
      <summary class="px-3 py-2 cursor-pointer flex items-center justify-between font-semibold text-secondaryDark">
        <span>SLA 压测判定阈值 (Verdict Assertions 可选)</span>
        <span class="text-tiny text-secondaryLight">p95 / p99 / 错误率 / QPS 达成率</span>
      </summary>
      <div class="p-3 border-t border-dividerLight grid grid-cols-2 md:grid-cols-3 gap-3">
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">p50 上限 (ms)</label>
          <input
            :value="assertSpec?.p50_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            placeholder="不限"
            class="input font-mono"
            @input="patchAssert('p50_ms', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">p95 上限 (ms)</label>
          <input
            :value="assertSpec?.p95_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            placeholder="例如 100"
            class="input font-mono"
            @input="patchAssert('p95_ms', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">p99 上限 (ms)</label>
          <input
            :value="assertSpec?.p99_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            placeholder="例如 250"
            class="input font-mono"
            @input="patchAssert('p99_ms', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">最大错误率 (0–1)</label>
          <input
            :value="assertSpec?.max_error_rate ?? ''"
            type="number"
            min="0"
            max="1"
            step="0.001"
            placeholder="例如 0.01"
            class="input font-mono"
            @input="patchAssert('max_error_rate', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">最低 QPS 达成率 (0–1)</label>
          <input
            :value="assertSpec?.min_qps_ratio ?? ''"
            type="number"
            min="0"
            max="1"
            step="0.05"
            placeholder="例如 0.9"
            class="input font-mono"
            @input="patchAssert('min_qps_ratio', ($event.target as HTMLInputElement).value)"
          />
        </div>
        <div>
          <label class="block text-tiny text-secondaryLight mb-1">Max 延迟上限 (ms)</label>
          <input
            :value="assertSpec?.max_ms ?? ''"
            type="number"
            min="0"
            step="1"
            placeholder="不限"
            class="input font-mono"
            @input="patchAssert('max_ms', ($event.target as HTMLInputElement).value)"
          />
        </div>
      </div>
    </details>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { HoppSmartToggle } from "@hoppscotch/ui";
import type { AssertSpec, LoadConfig } from "../../api/types";

const props = defineProps<{
  modelValue: LoadConfig;
  assertSpec?: AssertSpec | null;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: LoadConfig];
  "update:assertSpec": [value: AssertSpec | null];
}>();

const config = computed(() => props.modelValue);

const budgetKind = computed<"duration" | "requests">(() =>
  config.value.requests !== null && config.value.requests !== undefined
    ? "requests"
    : "duration",
);

function patch(partial: Partial<LoadConfig>): void {
  emit("update:modelValue", { ...config.value, ...partial });
}

function setModel(nextModel: string): void {
  if (nextModel === "closed_loop") {
    patch({ model: "closed_loop", qps: null });
  } else {
    patch({ model: "open_loop", qps: config.value.qps ?? 20 });
  }
}

function switchBudget(kind: "duration" | "requests"): void {
  if (kind === "duration") {
    patch({
      duration_secs: config.value.duration_secs ?? 10,
      requests: null,
    });
  } else {
    patch({
      requests: config.value.requests ?? 200,
      duration_secs: null,
    });
  }
}

function patchAssert(field: keyof AssertSpec, raw: string): void {
  const next: AssertSpec = { ...(props.assertSpec ?? {}) };
  const trimmed = raw.trim();
  if (trimmed === "") {
    delete next[field];
  } else {
    next[field] = Number(trimmed);
  }
  const hasAny = Object.values(next).some(
    (v) => v !== null && v !== undefined && !Number.isNaN(v),
  );
  emit("update:assertSpec", hasAny ? next : null);
}
</script>
