<template>
  <div class="flex min-h-0 flex-1 overflow-hidden bg-background">
    <!-- Left Sticky Tag & Endpoint Navigation -->
    <aside
      class="hidden lg:flex w-72 shrink-0 flex-col border-r border-border bg-surface/50 select-none"
    >
      <div class="space-y-2.5 border-b border-border p-3">
        <UiSearchInput
          v-model="searchQuery"
          placeholder="搜索路径、接口名称..."
        />

        <div class="flex flex-nowrap items-center gap-1">
          <button
            v-for="m in methodFilters"
            :key="m.value"
            type="button"
            class="inline-flex h-6 flex-auto cursor-pointer items-center justify-center rounded px-1.5 font-mono text-[10px] font-semibold whitespace-nowrap transition-colors"
            :class="
              activeMethodFilter === m.value
                ? 'bg-primary text-primary-foreground'
                : 'bg-muted text-muted-foreground hover:text-foreground'
            "
            @click="activeMethodFilter = m.value"
          >
            {{ m.label }}
          </button>
        </div>
      </div>

      <div class="flex-1 overflow-y-auto p-2.5 space-y-4">
        <div class="space-y-1">
          <button
            type="button"
            class="flex w-full cursor-pointer items-center justify-between rounded-md px-2 py-1 text-left text-xs font-semibold text-foreground transition-colors hover:bg-accent/60"
            :class="activeOpId === 'curl-manual' ? 'bg-accent text-foreground' : ''"
            @click="focusManual()"
          >
            <span class="truncate">Curl 手册</span>
            <span
              class="inline-flex h-5 min-w-[20px] items-center justify-center rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground"
            >
              免 WebUI
            </span>
          </button>
        </div>

        <div v-for="group in filteredGroups" :key="group.tag" class="space-y-1">
          <button
            type="button"
            class="flex w-full cursor-pointer items-center justify-between rounded-md px-2 py-1 text-left text-xs font-semibold text-foreground transition-colors hover:bg-accent/60"
            @click="scrollToAnchor(`tag-${group.tag}`)"
          >
            <span class="truncate">{{ group.title }}</span>
            <span
              class="inline-flex h-5 min-w-[20px] items-center justify-center rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground tabular-nums"
            >
              {{ group.operations.length }}
            </span>
          </button>

          <div class="space-y-0.5 pl-1">
            <button
              v-for="op in group.operations"
              :key="op.id"
              type="button"
              class="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent/60"
              :class="
                activeOpId === op.id
                  ? 'bg-accent text-foreground font-medium'
                  : 'text-muted-foreground hover:text-foreground'
              "
              @click="focusOperation(op.id)"
            >
              <span
                class="w-11 shrink-0 font-mono text-[10px] font-semibold"
                :class="methodColorClass(op.method)"
              >
                {{ op.method }}
              </span>
              <span class="truncate font-mono text-[11px]">{{ op.path }}</span>
            </button>
          </div>
        </div>
      </div>
    </aside>

    <!-- Main Scrollable Documentation Column -->
    <div class="min-w-0 flex-1 overflow-y-auto">
      <div v-if="loading" class="flex items-center justify-center py-24">
        <Loader2 class="h-6 w-6 animate-spin text-foreground" />
      </div>

      <div
        v-else-if="loadError"
        class="mx-auto max-w-3xl p-8 text-center space-y-3"
      >
        <p class="text-sm text-rose-600 dark:text-rose-400">{{ loadError }}</p>
        <UiButton variant="outline" size="sm" @click="fetchSpec">重试</UiButton>
      </div>

      <div v-else class="mx-auto max-w-5xl space-y-8 p-4 sm:p-6 lg:p-8">
        <!-- Hero Specification Header -->
        <UiCard class="space-y-4 p-4 sm:p-6">
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div class="min-w-0 space-y-1.5">
              <div class="flex flex-wrap items-center gap-2">
                <h1 class="text-xl font-semibold tracking-tight text-foreground">
                  {{ spec?.info?.title || "x-loadgen API" }}
                </h1>
                <UiBadge variant="outline" class="font-mono">
                  OpenAPI {{ spec?.openapi || "3.0" }}
                </UiBadge>
                <UiBadge variant="secondary" class="font-mono tabular-nums">
                  {{ totalOperationCount }} 个接口
                </UiBadge>
              </div>
              <p class="text-sm text-muted-foreground leading-relaxed max-w-3xl">
                先看下方「免 WebUI curl 调用手册」，用脚本完成导入、压测、调流与取报告；其后是按分组排列的 OpenAPI 参考，Mazbri 租户与密钥单独成类。所有接口均可一键载入工作台调试。
              </p>
            </div>

            <div class="flex flex-wrap items-center gap-2 shrink-0">
              <UiButton variant="outline" size="sm" @click="expandAllToggle">
                <ChevronsUpDown class="h-3.5 w-3.5" />
                <span>{{ allExpanded ? "收起全部" : "展开全部" }}</span>
              </UiButton>
              <UiButton variant="outline" size="sm" @click="downloadOpenApiJson">
                <Download class="h-3.5 w-3.5" />
                <span>导出 JSON</span>
              </UiButton>
            </div>
          </div>

          <!-- Auth Quick Reference Strip -->
          <div
            class="grid grid-cols-1 gap-3 rounded-lg border border-border bg-surface/60 p-4 text-xs md:grid-cols-3"
          >
            <div class="space-y-1">
              <div class="font-semibold text-foreground">服务地址</div>
              <div class="font-mono text-muted-foreground break-all">{{ originUrl }}</div>
            </div>
            <div class="space-y-1">
              <div class="font-semibold text-foreground">脚本鉴权</div>
              <div class="break-all font-mono text-muted-foreground">
                Authorization: Bearer &lt;管理员口令&gt;
              </div>
            </div>
            <div class="space-y-1">
              <div class="font-semibold text-foreground">浏览器会话</div>
              <div class="break-all font-mono text-muted-foreground">
                Cookie: x_loadgen_token=s_&lt;24h 会话 ID&gt;
              </div>
            </div>
          </div>
        </UiCard>

        <!-- Mobile Search Fallback -->
        <div class="lg:hidden">
          <UiSearchInput
            v-model="searchQuery"
            size="default"
            placeholder="搜索路径、接口名称或关键字..."
          />
        </div>

        <!-- Curl manual (always first) -->
        <section
          id="curl-manual"
          class="space-y-3 scroll-mt-6"
        >
          <div class="flex items-end justify-between gap-4 border-b border-border pb-2.5">
            <div class="min-w-0 space-y-0.5">
              <h2 class="text-base font-semibold tracking-tight text-foreground">
                x-loadgen 免 WebUI curl 调用手册
              </h2>
              <p class="text-xs text-muted-foreground">
                不打开工作台，用 curl 完成鉴权、导入、启动/管理压测任务与取结果。示例按真实请求体字段编写。
              </p>
            </div>
            <span class="shrink-0 font-mono text-xs text-muted-foreground">
              #curl
            </span>
          </div>

          <div class="space-y-3">
            <div
              v-for="block in curlManualBlocks"
              :id="block.id"
              :key="block.id"
              class="overflow-hidden rounded-lg border border-border bg-card"
            >
              <div class="space-y-1.5 border-b border-border px-4 py-3">
                <h3 class="text-sm font-semibold text-foreground">
                  {{ block.title }}
                </h3>
                <p class="text-xs text-muted-foreground leading-relaxed whitespace-pre-line">
                  {{ block.body }}
                </p>
              </div>
              <div class="space-y-2 p-4">
                <div class="flex h-6 items-center justify-between gap-2">
                  <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                    cURL
                  </span>
                  <button
                    type="button"
                    class="inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    @click="handleCopy(block.snippet, 'cURL 命令已复制')"
                  >
                    <Copy class="h-3 w-3" />
                    <span>复制 cURL</span>
                  </button>
                </div>
                <pre
                  class="max-h-72 overflow-auto rounded-lg border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap break-all"
                >{{ block.snippet }}</pre>
              </div>
            </div>
          </div>
        </section>

        <!-- Empty Search Result -->
        <div
          v-if="filteredGroups.length === 0"
          class="rounded-lg border border-dashed border-border py-16 text-center text-sm text-muted-foreground"
        >
          未找到匹配「{{ searchQuery }}」的接口
        </div>

        <!-- Tag Sections -->
        <section
          v-for="group in filteredGroups"
          :id="`tag-${group.tag}`"
          :key="group.tag"
          class="space-y-3 scroll-mt-6"
        >
          <div class="flex items-end justify-between gap-4 border-b border-border pb-2.5">
            <div class="min-w-0 space-y-0.5">
              <h2 class="text-base font-semibold tracking-tight text-foreground">
                {{ group.title }}
              </h2>
              <p class="text-xs text-muted-foreground">
                {{ group.subtitle }}
              </p>
            </div>
            <span class="shrink-0 font-mono text-xs text-muted-foreground">
              #{{ group.tag }}
            </span>
          </div>

          <!-- Endpoint Cards -->
          <div class="space-y-2.5">
            <div
              v-for="op in group.operations"
              :id="op.id"
              :key="op.id"
              class="overflow-hidden rounded-lg border border-border bg-card transition-colors scroll-mt-6"
            >
              <!-- Card Header Row: uniform h-6 controls -->
              <div
                class="flex flex-wrap items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-accent/35 transition-colors"
                @click="toggleExpand(op.id)"
              >
                <div class="flex min-w-0 flex-1 items-center gap-3">
                  <span
                    class="inline-flex h-6 w-16 shrink-0 items-center justify-center rounded border px-2 font-mono text-xs font-semibold"
                    :class="methodBadgeClass(op.method)"
                  >
                    {{ op.method }}
                  </span>

                  <code class="font-mono text-[13px] font-semibold text-foreground break-all">
                    {{ op.path }}
                  </code>

                  <span class="hidden sm:inline truncate text-xs text-muted-foreground">
                    {{ op.summary }}
                  </span>
                </div>

                <div class="flex shrink-0 items-center gap-2">
                  <span
                    class="hidden h-6 items-center gap-1 rounded-md border px-2 text-[11px] font-medium whitespace-nowrap md:inline-flex"
                    :class="
                      op.public
                        ? 'border-transparent bg-muted text-muted-foreground'
                        : 'border-border bg-background text-foreground'
                    "
                  >
                    <Unlock v-if="op.public" class="h-3 w-3 shrink-0" />
                    <Lock v-else class="h-3 w-3 shrink-0" />
                    <span>{{ op.public ? "公开免鉴权" : "需口令鉴权" }}</span>
                  </span>

                  <button
                    type="button"
                    title="载入工作台调试"
                    class="inline-flex h-6 cursor-pointer items-center gap-1 rounded-md border border-input bg-background px-2.5 text-xs font-medium whitespace-nowrap text-foreground shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground"
                    @click.stop="openInStudio(op)"
                  >
                    <span>调试</span>
                    <ArrowUpRight class="h-3 w-3" />
                  </button>

                  <button
                    type="button"
                    aria-label="展开或折叠接口详情"
                    class="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    @click.stop="toggleExpand(op.id)"
                  >
                    <ChevronDown
                      class="h-4 w-4 transition-transform"
                      :class="{ 'rotate-180': expandedIds.has(op.id) }"
                    />
                  </button>
                </div>
              </div>

              <!-- Expanded Details -->
              <div
                v-if="expandedIds.has(op.id)"
                class="space-y-5 border-t border-border bg-background/60 p-5 text-xs"
              >
                <div v-if="op.summary || op.description" class="space-y-1.5">
                  <div class="text-sm font-medium text-foreground">
                    {{ op.summary }}
                  </div>
                  <p
                    v-if="op.description"
                    class="text-xs text-muted-foreground leading-relaxed whitespace-pre-line"
                  >
                    {{ op.description }}
                  </p>
                </div>

                <!-- Path & Query Parameters -->
                <div v-if="op.parameters.length > 0" class="space-y-2">
                  <div class="flex h-6 items-center justify-between gap-2">
                    <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                      请求参数
                    </span>
                  </div>
                  <div class="overflow-x-auto rounded-lg border border-border bg-card">
                    <table class="w-full min-w-[520px] table-fixed border-collapse text-left">
                      <colgroup>
                        <col class="w-[22%]" />
                        <col class="w-[12%]" />
                        <col class="w-[20%]" />
                        <col class="w-[12%]" />
                        <col class="w-[34%]" />
                      </colgroup>
                      <thead>
                        <tr class="border-b border-border bg-muted/40 text-[11px] text-muted-foreground">
                          <th class="px-3 py-2 align-middle font-medium">参数名</th>
                          <th class="px-3 py-2 align-middle font-medium">位置</th>
                          <th class="px-3 py-2 align-middle font-medium">类型</th>
                          <th class="px-3 py-2 align-middle font-medium">必填</th>
                          <th class="px-3 py-2 align-middle font-medium">说明</th>
                        </tr>
                      </thead>
                      <tbody class="divide-y divide-border">
                        <tr v-for="p in op.parameters" :key="`${p.in}-${p.name}`">
                          <td class="px-3 py-2.5 align-middle font-mono font-medium text-foreground break-all">
                            {{ p.name }}
                          </td>
                          <td class="px-3 py-2.5 align-middle font-mono text-muted-foreground">
                            {{ p.in }}
                          </td>
                          <td class="px-3 py-2.5 align-middle font-mono text-muted-foreground break-all">
                            {{ p.typeLabel }}
                          </td>
                          <td class="px-3 py-2.5 align-middle">
                            <span
                              v-if="p.required"
                              class="inline-flex h-5 items-center rounded bg-primary px-1.5 text-[10px] font-medium text-primary-foreground"
                            >
                              必填
                            </span>
                            <span
                              v-else
                              class="inline-flex h-5 items-center rounded bg-muted px-1.5 text-[10px] font-medium text-muted-foreground"
                            >
                              可选
                            </span>
                          </td>
                          <td class="px-3 py-2.5 align-middle leading-5 text-muted-foreground">
                            {{ p.description || "-" }}
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>

                <!-- Request Body Schema & Sample -->
                <div v-if="op.requestBody" class="grid grid-cols-1 items-start gap-4 lg:grid-cols-2">
                  <div class="space-y-2 min-w-0">
                    <div class="flex h-6 items-center justify-between gap-2">
                      <span class="truncate text-xs font-semibold tracking-wide text-muted-foreground">
                        请求体结构（{{ op.requestBody.schemaName || op.requestBody.contentType }}）
                      </span>
                      <span
                        v-if="op.requestBody.required"
                        class="inline-flex h-5 shrink-0 items-center rounded border border-border bg-background px-1.5 text-[10px] font-medium text-foreground"
                      >
                        必填
                      </span>
                    </div>

                    <div
                      v-if="op.requestBody.fields.length > 0"
                      class="max-h-64 overflow-y-auto divide-y divide-border rounded-lg border border-border bg-card"
                    >
                      <div
                        v-for="f in op.requestBody.fields"
                        :key="f.name"
                        class="px-3 py-2.5 space-y-1"
                      >
                        <div class="flex items-baseline justify-between gap-2">
                          <span class="font-mono font-medium text-foreground">
                            {{ f.name }}
                            <span v-if="f.required" class="text-rose-500">*</span>
                          </span>
                          <span class="shrink-0 font-mono text-[11px] text-muted-foreground">
                            {{ f.type }}
                          </span>
                        </div>
                        <div v-if="f.description" class="text-[11px] leading-relaxed text-muted-foreground">
                          {{ f.description }}
                        </div>
                      </div>
                    </div>
                    <div
                      v-else
                      class="rounded-lg border border-border bg-card p-3 font-mono text-muted-foreground"
                    >
                      Content-Type: {{ op.requestBody.contentType }}
                    </div>
                  </div>

                  <div class="space-y-2 min-w-0">
                    <div class="flex h-6 items-center justify-between gap-2">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        请求 JSON 示例
                      </span>
                      <button
                        type="button"
                        class="inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                        @click="handleCopy(op.requestBody.sampleJson, '请求示例已复制')"
                      >
                        <Copy class="h-3 w-3" />
                        <span>复制</span>
                      </button>
                    </div>
                    <pre
                      class="max-h-64 overflow-auto rounded-lg border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap break-all"
                    >{{ op.requestBody.sampleJson }}</pre>
                  </div>
                </div>

                <!-- Responses + cURL Example -->
                <div class="grid grid-cols-1 items-start gap-4 lg:grid-cols-2">
                  <div class="space-y-2 min-w-0">
                    <div class="flex h-6 items-center justify-between gap-2">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        响应状态
                      </span>
                    </div>
                    <div class="divide-y divide-border rounded-lg border border-border bg-card">
                      <div
                        v-for="r in op.responses"
                        :key="r.code"
                        class="flex items-start justify-between gap-3 px-3 py-2.5"
                      >
                        <div class="flex min-w-0 flex-1 items-start gap-2.5">
                          <span
                            class="inline-flex h-5 w-11 shrink-0 items-center justify-center rounded border font-mono text-[11px] font-semibold tabular-nums"
                            :class="httpCodeBadge(r.code)"
                          >
                            {{ r.code }}
                          </span>
                          <span class="leading-5 text-foreground">{{ r.description }}</span>
                        </div>
                        <span
                          v-if="r.schemaLabel"
                          class="shrink-0 font-mono text-[11px] leading-5 text-muted-foreground"
                        >
                          {{ r.schemaLabel }}
                        </span>
                      </div>
                    </div>
                  </div>

                  <div class="space-y-2 min-w-0">
                    <div class="flex h-6 items-center justify-between gap-2">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        cURL 调用示例
                      </span>
                      <button
                        type="button"
                        class="inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                        @click="handleCopy(op.curlSnippet, 'cURL 命令已复制')"
                      >
                        <Copy class="h-3 w-3" />
                        <span>复制 cURL</span>
                      </button>
                    </div>
                    <pre
                      class="max-h-52 overflow-auto rounded-lg border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap break-all"
                    >{{ op.curlSnippet }}</pre>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import {
  ArrowUpRight,
  ChevronDown,
  ChevronsUpDown,
  Copy,
  Download,
  Loader2,
  Lock,
  Unlock,
} from "lucide-vue-next";
import { http } from "../api/http";
import {
  createDefaultRequestDraft,
  methodBadgeClass,
  methodColorClass,
} from "../helpers/format";
import { downloadBlob } from "../helpers/download";
import { toast } from "../modules/toast";
import { openDraftInNewTab } from "../stores/workspace";
import { copyText as writeClipboard } from "../utils/clipboard";
import UiBadge from "../components/ui/UiBadge.vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiSearchInput from "../components/ui/UiSearchInput.vue";

interface RawSchema {
  $ref?: string;
  type?: string;
  format?: string;
  description?: string;
  enum?: unknown[];
  required?: string[];
  properties?: Record<string, RawSchema>;
  items?: RawSchema;
  oneOf?: RawSchema[];
  anyOf?: RawSchema[];
  allOf?: RawSchema[];
  example?: unknown;
  default?: unknown;
}

interface RawParameter {
  name: string;
  in: string;
  required?: boolean;
  description?: string;
  schema?: RawSchema;
}

interface RawOperation {
  summary?: string;
  description?: string;
  tags?: string[];
  security?: unknown[];
  parameters?: RawParameter[];
  requestBody?: {
    required?: boolean;
    content?: Record<string, { schema?: RawSchema }>;
  };
  responses?: Record<
    string,
    {
      description?: string;
      content?: Record<string, { schema?: RawSchema }>;
    }
  >;
}

interface RawOpenApiDoc {
  openapi?: string;
  info?: {
    title?: string;
    version?: string;
    description?: string;
  };
  tags?: { name: string; description?: string }[];
  paths?: Record<string, Record<string, RawOperation | RawParameter[]>>;
  components?: {
    schemas?: Record<string, RawSchema>;
  };
}

interface SchemaFieldRow {
  name: string;
  type: string;
  required: boolean;
  description: string;
}

interface ParsedOperation {
  id: string;
  tag: string;
  method: string;
  path: string;
  summary: string;
  description: string;
  public: boolean;
  parameters: {
    name: string;
    in: string;
    typeLabel: string;
    required: boolean;
    description: string;
  }[];
  requestBody: {
    required: boolean;
    contentType: string;
    schemaName: string;
    fields: SchemaFieldRow[];
    sampleJson: string;
  } | null;
  responses: {
    code: string;
    description: string;
    schemaLabel: string;
  }[];
  curlSnippet: string;
}

const TAG_META: Record<string, { title: string; subtitle: string }> = {
  mazbri: {
    title: "Mazbri 租户与密钥",
    subtitle: "环境租户密钥池、请求绑定密钥对（持久化写入租户或无状态生成）。",
  },
  meta: {
    title: "系统与元信息",
    subtitle: "存活探针、实例运行规格与内置 OpenAPI 规范获取。",
  },
  auth: {
    title: "会话与口令鉴权",
    subtitle: "管理员口令登录、会话状态查询、口令轮换与登出。",
  },
  collections: {
    title: "接口集合",
    subtitle: "多层级接口集合目录树的增删改查及 JSON 批量导入导出。",
  },
  requests: {
    title: "已保存请求",
    subtitle: "HTTP 与 Mazbri 双协议请求配置的持久化归档管理。",
  },
  environments: {
    title: "环境与变量",
    subtitle: "全局/命名环境变量、HTTP 鉴权、TLS 以及环境导入导出。",
  },
  "load-profiles": {
    title: "负载预设",
    subtitle: "与具体接口解耦的可复用 QPS / 并发 / 爬坡 / SLA 判定预设。",
  },
  probe: {
    title: "单次接口探测",
    subtitle: "工作台「发送」按钮背后的实时 HTTP / Mazbri 单次探测与信封追踪。",
  },
  runs: {
    title: "压测任务与报告",
    subtitle: "单接口/集合加权压测编排、在线动态调流、SSE 实时指标、明细样本与离线报告。",
  },
};

const HTTP_VERBS = ["get", "post", "put", "patch", "delete"] as const;
const methodFilters = [
  { value: "ALL", label: "全部" },
  { value: "GET", label: "GET" },
  { value: "POST", label: "POST" },
  { value: "PUT", label: "PUT" },
  { value: "PATCH", label: "PATCH" },
  { value: "DELETE", label: "DELETE" },
] as const;

const router = useRouter();
const spec = ref<RawOpenApiDoc | null>(null);
const loading = ref(true);
const loadError = ref("");
const searchQuery = ref("");
const activeMethodFilter = ref<string>("ALL");
const activeOpId = ref<string>("");
const expandedIds = reactive(new Set<string>());


const curlManualBlocks = [
  {
    id: "curl-auth",
    title: "鉴权与公共接口",
    body: "默认需要鉴权。脚本推荐 Authorization: Bearer <管理员口令>：口令不自动过期，仅在改密或 reset-passwd 后失效。浏览器登录（POST /api/v1/auth/login，body 为 {token}）只 Set-Cookie x_loadgen_token=s_<64-hex>（HttpOnly，SameSite=Lax，Max-Age=86400），响应体不再回口令；Cookie 只接受会话 ID，不是口令本身。登出会吊销该会话，Bearer 口令仍然有效。首次启动必须先改密，否则业务接口返回 403 password_change_required。GET /health 与 GET /api/v1/openapi.json 无需认证。--no-auth 时把下面的 xlg 换成 curl -fsS -H 'Content-Type: application/json' 即可。不要给写请求加跨站 Sec-Fetch-Site。",
    snippet: "XLG=http://127.0.0.1:8757\nTOKEN_FILE=\\${TOKEN_FILE:-$HOME/.config/x-loadgen/token}\nHDR=\\${HDR:-$HOME/.config/x-loadgen/auth.hdr}\numask 077\nmkdir -p \"$(dirname \"$HDR\")\"\nprintf 'Authorization: Bearer %s\\\\nContent-Type: application/json\\\\n' \"$(tr -d '\\\\n' < \"$TOKEN_FILE\")\" > \"$HDR\"\nxlg() { curl -fsS -H @\"$HDR\" \"$@\"; }\n\ncurl -fsS \"$XLG/health\"\ncurl -fsS \"$XLG/api/v1/openapi.json\" | jq '{openapi, title: .info.title}'\nxlg \"$XLG/api/v1/auth/me\" | jq .\nxlg \"$XLG/api/v1/meta\" | jq .",
  },
  {
    id: "curl-import",
    title: "导入集合与环境",
    body: "POST /api/v1/collections/import 接受 x-loadgen 原生集合 JSON 或 Hoppscotch REST Collection JSON，可选 query parent_id 挂到已有目录下。POST /api/v1/environments/import 接受原生或 Hoppscotch Environment JSON。导出默认脱敏，需要明文时加 ?include_secrets=true。工作台「导入 cURL」只在浏览器内解析，没有对应的 HTTP 导入接口。",
    snippet: "# 导入先前导出的集合 / 环境文件\nxlg -X POST \"$XLG/api/v1/collections/import\" --data-binary @collections.json | jq 'length'\n# xlg -X POST \"$XLG/api/v1/collections/import?parent_id=$COLL_ID\" --data-binary @collections.json\n\nxlg -X POST \"$XLG/api/v1/environments/import\" --data-binary @environments.json | jq '[.[].name]'\n\n# 导出（默认脱敏）\nxlg \"$XLG/api/v1/collections/export\" -o collections.json\nxlg \"$XLG/api/v1/environments/export?include_secrets=true\" -o environments.json",
  },
  {
    id: "curl-run-adhoc",
    title: "启动压测（已保存请求 / 集合 / adhoc）",
    body: "POST /api/v1/runs 的目标三选一：request_id、collection_id（可选 selected_requests 覆盖权重）、或 adhoc_request / adhoc_requests（不保存直接压）。可指定 environment_id、load_profile / load_profile_id，以及本次覆盖字段（qps、concurrency、requests、duration_secs、warmup_secs、ramp_secs、processes、mode、assert 等，只作用于本次 snapshot）。requests 与 duration_secs 互斥。?wait=true&timeout=N（1–600，默认 60）会等到任务离开 running/stopping 再返回 201；超时 409 wait_timeout。",
    snippet: "# 对已保存请求按预设启动一次压测，并等待结束\nRUN=$(xlg -X POST \"$XLG/api/v1/runs?wait=true&timeout=60\" --data-binary @- <<EOF\n{\n  \"request_id\": \"$REQ_ID\",\n  \"environment_id\": \"$ENV_ID\",\n  \"load_profile\": \"nightly-once\",\n  \"mode\": \"once\",\n  \"qps\": 15,\n  \"concurrency\": 4,\n  \"requests\": 30,\n  \"processes\": 1,\n  \"assert\": {\"p99_ms\": 5000, \"max_error_rate\": 0.05, \"min_qps_ratio\": 0.5}\n}\nEOF\n)\nRUN_ID=$(echo \"$RUN\" | jq -r .id)\n\n# 不保存请求，直接 adhoc 压测\nADHOC=$(xlg -X POST \"$XLG/api/v1/runs?wait=true&timeout=60\" --data-binary @- <<'EOF'\n{\n  \"adhoc_request\": {\n    \"name\": \"adhoc-ping\",\n    \"protocol\": \"http\",\n    \"method\": \"GET\",\n    \"endpoint\": \"http://127.0.0.1:9/health\",\n    \"headers\": [],\n    \"params\": [],\n    \"body\": {\"content_type\": \"application/json\", \"body\": \"\", \"form\": []},\n    \"assertions\": {\"expect_transport\": \"ok\", \"checks\": []}\n  },\n  \"mode\": \"once\",\n  \"qps\": 5,\n  \"concurrency\": 2,\n  \"requests\": 10,\n  \"timeout_secs\": 3\n}\nEOF\n)\necho \"$ADHOC\" | jq '{id, status, verdict}'\n\n# 集合加权循环压测（立即返回，稍后 stop）\nLOOP=$(xlg -X POST \"$XLG/api/v1/runs\" --data-binary @- <<EOF\n{\n  \"collection_id\": \"$COLL_ID\",\n  \"selected_requests\": [\n    {\"request_id\": \"$REQ_ID\", \"weight\": 3}\n  ],\n  \"environment_id\": \"$ENV_ID\",\n  \"mode\": \"loop\",\n  \"qps\": 8,\n  \"rounds\": 50,\n  \"loop_interval_secs\": 2\n}\nEOF\n)\nLOOP_ID=$(echo \"$LOOP\" | jq -r .id)",
  },
  {
    id: "curl-manage",
    title: "任务状态、调流、停止与续跑",
    body: "GET /api/v1/runs?status=running|finished|stopped|failed 列出任务。GET /api/v1/runs/{id} 含脱敏 snapshot 与 verdict。PATCH /api/v1/runs/{id}/traffic 仅对 running 有效，qps 与 concurrency 都可选。POST .../stop 平滑停止；POST .../resume 对已停止或异常中断的任务断点续跑。DELETE 只能删除已结束或已停止的任务。",
    snippet: "xlg \"$XLG/api/v1/runs?status=running\" | jq '[.[] | {id, status, current_qps}]'\nxlg \"$XLG/api/v1/runs/$LOOP_ID\" | jq '{id, status, verdict}'\nxlg \"$XLG/api/v1/runs/$LOOP_ID/stats\" | jq '{status, samples, success, failure, p99_us, current_qps}'\nxlg -X PATCH \"$XLG/api/v1/runs/$LOOP_ID/traffic\" --data-binary '{\"qps\":20,\"concurrency\":4}' \\\\\n  | jq '{id, status, current_qps, current_concurrency}'\nxlg -X POST \"$XLG/api/v1/runs/$LOOP_ID/stop\" | jq '{id, status}'\n# xlg -X POST \"$XLG/api/v1/runs/$LOOP_ID/resume\" | jq '{id, status}'\n",
  },
  {
    id: "curl-events-report",
    title: "SSE 实时事件、报告与判定",
    body: "GET /api/v1/runs/{id}/events 是 SSE，事件名为 stats，约每秒一条；curl -N 关闭缓冲，Ctrl-C 结束。GET .../report 返回脱敏完整报告 JSON；.../report.json 与 .../report.html 为附件下载。GET .../verdict 的 HTTP 状态是为了让 curl -f 当脚本退出码：通过为 200，失败为 4xx/5xx（有意不用 202）。",
    snippet: "# SSE（另开终端；Ctrl-C 结束）\n# curl -NS -H @\"$HDR\" \"$XLG/api/v1/runs/$LOOP_ID/events\"\n\nxlg \"$XLG/api/v1/runs/$RUN_ID/timeseries\" | jq '{count: (.points | length)}'\nxlg \"$XLG/api/v1/runs/$RUN_ID/timeline\" | jq '.events[] | {ts, event}'\nxlg \"$XLG/api/v1/runs/$RUN_ID/report\" | jq '{id, status, counters, verdict}'\nxlg -o \"run-$RUN_ID-report.json\" \"$XLG/api/v1/runs/$RUN_ID/report.json\"\ncurl -fsS -H @\"$HDR\" -o \"run-$RUN_ID-report.html\" \"$XLG/api/v1/runs/$RUN_ID/report.html\"\n\nxlg \"$XLG/api/v1/runs/$RUN_ID/verdict\" | jq '{result, actual, failed}'\ncode=$(curl -sS -o /tmp/verdict.json -w '%{http_code}' -H @\"$HDR\" \\\\\n  \"$XLG/api/v1/runs/$RUN_ID/verdict\")\necho \"$code\"\n",
  },
];

const originUrl = computed(() =>
  typeof window !== "undefined" ? window.location.origin : "http://127.0.0.1:8757",
);

function refName(refStr?: string): string {
  if (!refStr) return "";
  const parts = refStr.split("/");
  return parts[parts.length - 1] || "";
}

function resolveSchema(
  raw: RawSchema | undefined,
  schemas: Record<string, RawSchema>,
  depth = 0,
): RawSchema {
  if (!raw || depth > 8) return {};
  if (raw.$ref) {
    const name = refName(raw.$ref);
    const target = schemas[name];
    if (target) return resolveSchema(target, schemas, depth + 1);
  }
  if (raw.allOf && raw.allOf.length > 0) {
    const merged: RawSchema = { type: "object", properties: {}, required: [] };
    for (const sub of raw.allOf) {
      const r = resolveSchema(sub, schemas, depth + 1);
      if (r.properties) {
        merged.properties = { ...merged.properties, ...r.properties };
      }
      if (r.required) {
        merged.required = [...(merged.required ?? []), ...r.required];
      }
    }
    return merged;
  }
  if (raw.oneOf && raw.oneOf[0]) {
    return resolveSchema(raw.oneOf[0], schemas, depth + 1);
  }
  if (raw.anyOf && raw.anyOf[0]) {
    return resolveSchema(raw.anyOf[0], schemas, depth + 1);
  }
  return raw;
}

function formatSchemaType(
  raw: RawSchema | undefined,
  schemas: Record<string, RawSchema>,
): string {
  if (!raw) return "any";
  if (raw.$ref) return refName(raw.$ref);
  if (raw.enum && raw.enum.length > 0) {
    return raw.enum.map((v) => JSON.stringify(v)).join(" | ");
  }
  if (raw.type === "array") {
    return `${formatSchemaType(raw.items, schemas)}[]`;
  }
  if (raw.format) {
    return `${raw.type ?? "string"} (${raw.format})`;
  }
  return raw.type || "object";
}

function buildSampleValue(
  raw: RawSchema | undefined,
  schemas: Record<string, RawSchema>,
  visited = new Set<string>(),
  depth = 0,
): unknown {
  if (!raw || depth > 4) return {};
  if (raw.example !== undefined) return raw.example;
  if (raw.default !== undefined) return raw.default;
  if (raw.$ref) {
    const name = refName(raw.$ref);
    if (visited.has(name)) return {};
    const nextVisited = new Set(visited);
    nextVisited.add(name);
    return buildSampleValue(schemas[name], schemas, nextVisited, depth + 1);
  }
  const resolved = resolveSchema(raw, schemas, depth);
  if (resolved.enum && resolved.enum.length > 0) {
    return resolved.enum[0];
  }
  switch (resolved.type) {
    case "string":
      if (resolved.format === "uuid") return "0196ed00-0000-7000-8000-000000000001";
      return "string";
    case "integer":
    case "number":
      return 1;
    case "boolean":
      return true;
    case "array":
      return [buildSampleValue(resolved.items, schemas, visited, depth + 1)];
    case "object":
    default: {
      if (!resolved.properties) return {};
      const obj: Record<string, unknown> = {};
      const entries = Object.entries(resolved.properties).slice(0, 8);
      for (const [k, prop] of entries) {
        obj[k] = buildSampleValue(prop, schemas, visited, depth + 1);
      }
      return obj;
    }
  }
}

function extractFields(
  raw: RawSchema | undefined,
  schemas: Record<string, RawSchema>,
): SchemaFieldRow[] {
  const resolved = resolveSchema(raw, schemas);
  if (!resolved.properties) return [];
  const reqSet = new Set(resolved.required ?? []);
  return Object.entries(resolved.properties).map(([name, prop]) => ({
    name,
    type: formatSchemaType(prop, schemas),
    required: reqSet.has(name),
    description: prop.description ?? "",
  }));
}

const allOperations = computed<ParsedOperation[]>(() => {
  const doc = spec.value;
  if (!doc?.paths) return [];
  const schemas = doc.components?.schemas ?? {};
  const list: ParsedOperation[] = [];

  for (const [path, pathItem] of Object.entries(doc.paths)) {
    const sharedParams = Array.isArray(pathItem.parameters)
      ? (pathItem.parameters as RawParameter[])
      : [];

    for (const verb of HTTP_VERBS) {
      const op = pathItem[verb] as RawOperation | undefined;
      if (!op || Array.isArray(op)) continue;

      const method = verb.toUpperCase();
      const tag = op.tags?.[0] || "meta";
      const id = `op-${method.toLowerCase()}-${path.replace(/[^a-zA-Z0-9]/g, "-")}`;
      const isPublic = Array.isArray(op.security) && op.security.length === 0;

      const mergedParams = [...sharedParams, ...(op.parameters ?? [])];
      const parameters = mergedParams.map((p) => ({
        name: p.name,
        in: p.in,
        typeLabel: formatSchemaType(p.schema, schemas),
        required: Boolean(p.required),
        description: p.description ?? "",
      }));

      let requestBody: ParsedOperation["requestBody"] = null;
      let rawSampleValue: unknown = undefined;
      const contentMap = op.requestBody?.content;
      if (contentMap) {
        const ct =
          Object.keys(contentMap).find((k) => k.includes("json")) ||
          Object.keys(contentMap)[0];
        if (ct && contentMap[ct]) {
          const rawSchema = contentMap[ct]?.schema;
          const sample = buildSampleValue(rawSchema, schemas);
          rawSampleValue = sample;
          requestBody = {
            required: Boolean(op.requestBody?.required),
            contentType: ct,
            schemaName: rawSchema?.$ref ? refName(rawSchema.$ref) : "",
            fields: extractFields(rawSchema, schemas),
            sampleJson: JSON.stringify(sample, null, 2),
          };
        }
      }

      const responses = Object.entries(op.responses ?? {}).map(([code, r]) => {
        const jsonSchema = r.content?.["application/json"]?.schema;
        let schemaLabel = "";
        if (jsonSchema) {
          schemaLabel = formatSchemaType(jsonSchema, schemas);
        } else if (r.content) {
          schemaLabel = Object.keys(r.content)[0] ?? "";
        }
        return {
          code,
          description: r.description ?? "",
          schemaLabel,
        };
      });

      const curlLines = [`curl -X ${method} '${originUrl.value}${path}'`];
      if (!isPublic) {
        curlLines.push(`  -H 'Authorization: Bearer <ADMIN_TOKEN>'`);
      }
      if (requestBody) {
        curlLines.push(`  -H 'Content-Type: ${requestBody.contentType}'`);
        const compactJson = JSON.stringify(rawSampleValue ?? {}).replace(
          /'/g,
          "'\\''",
        );
        curlLines.push(`  -d '${compactJson}'`);
      }

      list.push({
        id,
        tag,
        method,
        path,
        summary: op.summary ?? path,
        description: op.description ?? "",
        public: isPublic,
        parameters,
        requestBody,
        responses,
        curlSnippet: curlLines.join(" \\\n"),
      });
    }
  }

  return list;
});

const totalOperationCount = computed(() => allOperations.value.length);

const filteredGroups = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  const mf = activeMethodFilter.value;
  const tagOrder = Object.keys(TAG_META);

  const map = new Map<string, ParsedOperation[]>();
  for (const op of allOperations.value) {
    if (mf !== "ALL" && op.method !== mf) continue;
    if (
      q &&
      !op.path.toLowerCase().includes(q) &&
      !op.summary.toLowerCase().includes(q) &&
      !op.description.toLowerCase().includes(q) &&
      !op.tag.toLowerCase().includes(q)
    ) {
      continue;
    }
    const bucket = map.get(op.tag) ?? [];
    bucket.push(op);
    map.set(op.tag, bucket);
  }

  const keys = Array.from(map.keys()).sort((a, b) => {
    const ia = tagOrder.indexOf(a);
    const ib = tagOrder.indexOf(b);
    return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
  });

  return keys.map((tag) => ({
    tag,
    title: TAG_META[tag]?.title ?? tag,
    subtitle: TAG_META[tag]?.subtitle ?? "",
    operations: map.get(tag) ?? [],
  }));
});

const allExpanded = computed(
  () =>
    allOperations.value.length > 0 &&
    expandedIds.size >= allOperations.value.length,
);

function httpCodeBadge(code: string): string {
  if (code.startsWith("2")) {
    return "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
  }
  if (code.startsWith("4")) {
    return "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400";
  }
  if (code.startsWith("5")) {
    return "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-400";
  }
  return "border-border bg-muted text-foreground";
}

function toggleExpand(id: string): void {
  if (expandedIds.has(id)) {
    expandedIds.delete(id);
  } else {
    expandedIds.add(id);
  }
}

function expandAllToggle(): void {
  if (allExpanded.value) {
    expandedIds.clear();
  } else {
    for (const op of allOperations.value) {
      expandedIds.add(op.id);
    }
  }
}

function scrollToAnchor(domId: string): void {
  const el = document.getElementById(domId);
  el?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function focusManual(): void {
  activeOpId.value = "curl-manual";
  scrollToAnchor("curl-manual");
}

function focusOperation(opId: string): void {
  activeOpId.value = opId;
  expandedIds.add(opId);
  scrollToAnchor(opId);
}

function openInStudio(op: ParsedOperation): void {
  const draft = createDefaultRequestDraft("http");
  draft.name = op.summary || `${op.method} ${op.path}`;
  draft.method = op.method;
  const templatedPath = op.path.replace(
    /\{([^}]+)\}/g,
    (_, paramName: string) => `<<${paramName.trim()}>>`,
  );
  draft.endpoint = `${originUrl.value}${templatedPath}`;
  draft.params = op.parameters
    .filter((p) => p.in === "query")
    .map((p) => ({
      key: p.name,
      value: "",
      active: p.required,
      description: p.description,
    }));
  draft.headers = op.parameters
    .filter((p) => p.in === "header")
    .map((p) => ({
      key: p.name,
      value: "",
      active: p.required,
      description: p.description,
    }));
  if (op.requestBody) {
    draft.body = {
      content_type: op.requestBody.contentType,
      body: op.requestBody.sampleJson,
      form: [],
    };
  }
  openDraftInNewTab(draft);
  toast.success(`已在工作台打开「${op.method} ${op.path}」`);
  void router.push({ name: "workspace" });
}

async function handleCopy(text: string, msg: string): Promise<void> {
  const ok = await writeClipboard(text);
  if (ok) {
    toast.success(msg);
  } else {
    toast.error("复制失败");
  }
}

function downloadOpenApiJson(): void {
  if (!spec.value) return;
  const blob = new Blob([JSON.stringify(spec.value, null, 2)], {
    type: "application/json",
  });
  downloadBlob(blob, "x-loadgen-openapi.json");
  toast.success("已下载 openapi.json");
}

async function fetchSpec(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    const { data } = await http.get<RawOpenApiDoc>("/api/v1/openapi.json");
    spec.value = data;
    expandedIds.clear();
    activeOpId.value = "";
    // Expand the first two operations in the main content area without marking left nav items as selected
    for (const op of allOperations.value.slice(0, 2)) {
      expandedIds.add(op.id);
    }
  } catch (err) {
    loadError.value = `无法加载 OpenAPI 规范：${err instanceof Error ? err.message : String(err)}`;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void fetchSpec();
});
</script>
