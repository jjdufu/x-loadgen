<template>
  <div class="space-y-4">
    <!-- Core Model & Concurrency -->
    <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          发压模型
        </label>
        <UiSelect
          :model-value="config.model"
          @update:model-value="setModel"
        >
          <option value="open_loop">恒定速率 (固定 QPS)</option>
          <option value="closed_loop">并发饱和 (闭环)</option>
        </UiSelect>
      </div>

      <div v-if="config.model === 'open_loop'" class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          目标 QPS *
        </label>
        <UiInput
          :model-value="config.qps ?? 20"
          type="number"
          min="1"
          mono
          @update:model-value="(v) => patch({ qps: Math.max(1, Number(v) || 1) })"
        />
      </div>

      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
          最大并发数 *
        </label>
        <UiInput
          :model-value="config.concurrency"
          type="number"
          min="1"
          mono
          @update:model-value="(v) => patch({ concurrency: Math.max(1, Number(v) || 1) })"
        />
      </div>
    </div>

    <!-- Budget XOR: Duration vs Total Requests -->
    <div class="rounded-lg border border-border bg-surface/50 p-4 space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <span class="text-sm font-semibold text-foreground">
          单轮压测预算
        </span>
        <div class="inline-flex h-8 items-center rounded-md border border-border bg-muted p-0.5">
          <button
            type="button"
            class="h-full cursor-pointer rounded px-3 text-xs font-medium whitespace-nowrap transition-all"
            :class="
              budgetKind === 'duration'
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            "
            @click="switchBudget('duration')"
          >
            按持续时长
          </button>
          <button
            type="button"
            class="h-full cursor-pointer rounded px-3 text-xs font-medium whitespace-nowrap transition-all"
            :class="
              budgetKind === 'requests'
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            "
            @click="switchBudget('requests')"
          >
            按请求总数
          </button>
        </div>
      </div>

      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
        <div v-if="budgetKind === 'duration'" class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            持续时长 (秒) *
          </label>
          <UiInput
            :model-value="config.duration_secs ?? 10"
            type="number"
            min="1"
            mono
            @update:model-value="
              (v) =>
                patch({
                  duration_secs: Math.max(1, Number(v) || 1),
                  requests: null,
                })
            "
          />
        </div>
        <div v-else class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            请求总数 *
          </label>
          <UiInput
            :model-value="config.requests ?? 100"
            type="number"
            min="1"
            mono
            @update:model-value="
              (v) =>
                patch({
                  requests: Math.max(1, Number(v) || 1),
                  duration_secs: null,
                })
            "
          />
        </div>

        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            预热时长 (秒)
          </label>
          <UiInput
            :model-value="config.warmup_secs"
            type="number"
            min="0"
            mono
            @update:model-value="(v) => patch({ warmup_secs: Math.max(0, Number(v) || 0) })"
          />
        </div>

        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            线性爬坡 (秒)
          </label>
          <UiInput
            :model-value="config.ramp_secs"
            type="number"
            min="0"
            mono
            @update:model-value="(v) => patch({ ramp_secs: Math.max(0, Number(v) || 0) })"
          />
        </div>
      </div>
    </div>

    <!-- Advanced Multi-Process, Loop & Timeouts -->
    <details class="group rounded-lg border border-border bg-card">
      <summary
        class="flex cursor-pointer items-center justify-between px-4 py-3 text-sm font-medium text-foreground select-none"
      >
        <span>集群进程、循环与超时配置</span>
        <span class="font-mono text-xs text-muted-foreground tabular-nums">
          {{ config.processes }} 进程 · 超时 {{ config.timeout_secs }}s
        </span>
      </summary>
      <div class="space-y-4 border-t border-border p-4">
        <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              工作进程数 (1–64)
            </label>
            <UiInput
              :model-value="config.processes"
              type="number"
              min="1"
              max="64"
              mono
              @update:model-value="
                (v) => patch({ processes: Math.min(64, Math.max(1, Number(v) || 1)) })
              "
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              QPS 分配模式
            </label>
            <UiSelect
              :model-value="config.qps_mode"
              @update:model-value="(v) => patch({ qps_mode: v })"
            >
              <option value="total">全局均摊</option>
              <option value="per_process">每进程独立</option>
            </UiSelect>
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              请求超时 (秒)
            </label>
            <UiInput
              :model-value="config.timeout_secs"
              type="number"
              min="1"
              mono
              @update:model-value="(v) => patch({ timeout_secs: Math.max(1, Number(v) || 30) })"
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              连接超时 (秒)
            </label>
            <UiInput
              :model-value="config.connect_timeout_secs"
              type="number"
              min="1"
              mono
              @update:model-value="
                (v) => patch({ connect_timeout_secs: Math.max(1, Number(v) || 5) })
              "
            />
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-6 pt-1">
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch
              :model-value="config.loop_enabled"
              @update:model-value="(v) => patch({ loop_enabled: v })"
            />
            <span>启用多轮循环压测</span>
          </label>
          <label class="inline-flex cursor-pointer items-center gap-2.5 text-sm text-foreground">
            <UiSwitch
              :model-value="config.include_request_id"
              @update:model-value="(v) => patch({ include_request_id: v })"
            />
            <span>自动注入 X-Request-ID 追踪头</span>
          </label>
        </div>

        <div v-if="config.loop_enabled" class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              循环轮数（留空不限轮数）
            </label>
            <UiInput
              :model-value="config.rounds ?? ''"
              type="number"
              min="1"
              mono
              placeholder="不限轮数"
              @update:model-value="
                (v) =>
                  patch({
                    rounds: v ? Math.max(1, Number(v)) : null,
                  })
              "
            />
          </div>

          <div class="space-y-1.5">
            <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
              轮间间隔 (秒)
            </label>
            <UiInput
              :model-value="config.loop_interval_secs"
              type="number"
              min="0"
              mono
              @update:model-value="
                (v) => patch({ loop_interval_secs: Math.max(0, Number(v) || 0) })
              "
            />
          </div>
        </div>
      </div>
    </details>

    <!-- SLA Assertions -->
    <details class="group rounded-lg border border-border bg-card">
      <summary
        class="flex cursor-pointer items-center justify-between px-4 py-3 text-sm font-medium text-foreground select-none"
      >
        <span>SLA 判定阈值（可选）</span>
        <span class="text-xs text-muted-foreground">
          p95 / p99 / 错误率 / QPS 达成率
        </span>
      </summary>
      <div class="grid grid-cols-2 gap-4 border-t border-border p-4 md:grid-cols-3">
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            p50 上限 (ms)
          </label>
          <UiInput
            :model-value="assertSpec?.p50_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            mono
            placeholder="不限"
            @update:model-value="(v) => patchAssert('p50_ms', v)"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            p95 上限 (ms)
          </label>
          <UiInput
            :model-value="assertSpec?.p95_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            mono
            placeholder="例如 100"
            @update:model-value="(v) => patchAssert('p95_ms', v)"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            p99 上限 (ms)
          </label>
          <UiInput
            :model-value="assertSpec?.p99_ms ?? ''"
            type="number"
            min="0"
            step="0.1"
            mono
            placeholder="例如 250"
            @update:model-value="(v) => patchAssert('p99_ms', v)"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            最大错误率 (0–1)
          </label>
          <UiInput
            :model-value="assertSpec?.max_error_rate ?? ''"
            type="number"
            min="0"
            max="1"
            step="0.001"
            mono
            placeholder="例如 0.01"
            @update:model-value="(v) => patchAssert('max_error_rate', v)"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            最低 QPS 达成率 (0–1)
          </label>
          <UiInput
            :model-value="assertSpec?.min_qps_ratio ?? ''"
            type="number"
            min="0"
            max="1"
            step="0.05"
            mono
            placeholder="例如 0.9"
            @update:model-value="(v) => patchAssert('min_qps_ratio', v)"
          />
        </div>
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-muted-foreground whitespace-nowrap">
            Max 延迟上限 (ms)
          </label>
          <UiInput
            :model-value="assertSpec?.max_ms ?? ''"
            type="number"
            min="0"
            step="1"
            mono
            placeholder="不限"
            @update:model-value="(v) => patchAssert('max_ms', v)"
          />
        </div>
      </div>
    </details>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { AssertSpec, LoadConfig } from "../../api/types";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiSwitch from "../ui/UiSwitch.vue";

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
