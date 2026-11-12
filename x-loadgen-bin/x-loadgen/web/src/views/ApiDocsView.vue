<template>
  <div class="flex min-h-0 flex-1 overflow-hidden bg-background">
    <!-- Left Sticky Tag & Endpoint Navigation -->
    <aside
      class="hidden lg:flex w-72 shrink-0 flex-col border-r border-border bg-surface/50 select-none"
    >
      <div class="space-y-2.5 border-b border-border p-3.5">
        <UiSearchInput
          v-model="searchQuery"
          placeholder="搜索路径、接口名称..."
        />

        <div class="flex flex-wrap items-center gap-1">
          <button
            v-for="m in methodFilters"
            :key="m.value"
            type="button"
            class="rounded px-2 py-0.5 font-mono text-[10px] font-semibold transition-colors"
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
        <div v-for="group in filteredGroups" :key="group.tag" class="space-y-1">
          <button
            type="button"
            class="flex w-full items-center justify-between rounded-md px-2 py-1 text-left text-xs font-semibold text-foreground hover:bg-accent/60"
            @click="scrollToAnchor(`tag-${group.tag}`)"
          >
            <span class="truncate">{{ group.title }}</span>
            <span
              class="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-muted-foreground tabular-nums"
            >
              {{ group.operations.length }}
            </span>
          </button>

          <div class="space-y-0.5 pl-1">
            <button
              v-for="op in group.operations"
              :key="op.id"
              type="button"
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent/60"
              :class="
                expandedIds.has(op.id)
                  ? 'bg-accent/50 text-foreground font-medium'
                  : 'text-muted-foreground'
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

      <div v-else class="mx-auto max-w-5xl space-y-8 p-6 lg:p-8">
        <!-- Hero Specification Header -->
        <UiCard class="p-6 space-y-4">
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div class="space-y-1.5">
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
                内置离线接口规范文档，涵盖集合管理、环境与 Mazbri 租户密钥池、单次双协议探测及多进程压测生命周期控制，所有接口均可一键载入工作台调试。
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
              <div class="font-mono text-muted-foreground">
                Authorization: Bearer &lt;token&gt;
              </div>
            </div>
            <div class="space-y-1">
              <div class="font-semibold text-foreground">会话鉴权</div>
              <div class="font-mono text-muted-foreground">
                Cookie: x_loadgen_token=&lt;token&gt;
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
          <div class="flex items-baseline justify-between border-b border-border pb-2">
            <div>
              <h2 class="text-base font-semibold tracking-tight text-foreground">
                {{ group.title }}
              </h2>
              <p class="text-xs text-muted-foreground mt-0.5">
                {{ group.subtitle }}
              </p>
            </div>
            <span class="font-mono text-xs text-muted-foreground">
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
              <!-- Card Header Row -->
              <div
                class="flex flex-wrap items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-accent/35 transition-colors"
                @click="toggleExpand(op.id)"
              >
                <div class="flex min-w-0 flex-1 items-center gap-3">
                  <span
                    class="inline-flex w-16 shrink-0 items-center justify-center rounded border px-2 py-0.5 font-mono text-xs font-semibold"
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

                <div class="flex shrink-0 items-center gap-2" @click.stop>
                  <UiBadge :variant="op.public ? 'muted' : 'outline'" class="hidden md:inline-flex">
                    <Unlock v-if="op.public" class="h-3 w-3" />
                    <Lock v-else class="h-3 w-3" />
                    <span>{{ op.public ? "公开免鉴权" : "需口令鉴权" }}</span>
                  </UiBadge>

                  <UiButton
                    variant="outline"
                    size="xs"
                    title="载入工作台调试"
                    @click="openInStudio(op)"
                  >
                    <span>调试</span>
                    <ArrowUpRight class="h-3 w-3" />
                  </UiButton>

                  <button
                    type="button"
                    aria-label="展开或折叠接口详情"
                    class="rounded p-1 text-muted-foreground hover:text-foreground"
                    @click="toggleExpand(op.id)"
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
                  <div class="text-xs font-semibold tracking-wide text-muted-foreground">
                    请求参数
                  </div>
                  <div class="overflow-x-auto rounded-lg border border-border">
                    <table class="w-full border-collapse text-left">
                      <thead>
                        <tr class="border-b border-border bg-muted/40 text-[11px] text-muted-foreground">
                          <th class="px-3 py-2 font-medium">参数名</th>
                          <th class="px-3 py-2 font-medium">位置</th>
                          <th class="px-3 py-2 font-medium">类型</th>
                          <th class="px-3 py-2 font-medium">必填</th>
                          <th class="px-3 py-2 font-medium">说明</th>
                        </tr>
                      </thead>
                      <tbody class="divide-y divide-border">
                        <tr v-for="p in op.parameters" :key="`${p.in}-${p.name}`">
                          <td class="px-3 py-2 font-mono font-medium text-foreground">
                            {{ p.name }}
                          </td>
                          <td class="px-3 py-2 font-mono text-muted-foreground">
                            {{ p.in }}
                          </td>
                          <td class="px-3 py-2 font-mono text-muted-foreground">
                            {{ p.typeLabel }}
                          </td>
                          <td class="px-3 py-2">
                            <UiBadge v-if="p.required" variant="default" class="text-[10px]">
                              必填
                            </UiBadge>
                            <span v-else class="text-muted-foreground">可选</span>
                          </td>
                          <td class="px-3 py-2 text-muted-foreground">
                            {{ p.description || "-" }}
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>

                <!-- Request Body Schema & Sample -->
                <div v-if="op.requestBody" class="grid grid-cols-1 gap-4 lg:grid-cols-2">
                  <div class="space-y-2">
                    <div class="flex items-center justify-between">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        请求体结构（{{ op.requestBody.schemaName || op.requestBody.contentType }}）
                      </span>
                      <UiBadge v-if="op.requestBody.required" variant="outline">
                        必填
                      </UiBadge>
                    </div>

                    <div
                      v-if="op.requestBody.fields.length > 0"
                      class="max-h-64 overflow-y-auto divide-y divide-border rounded-lg border border-border bg-card"
                    >
                      <div
                        v-for="f in op.requestBody.fields"
                        :key="f.name"
                        class="px-3 py-2 space-y-0.5"
                      >
                        <div class="flex items-center justify-between gap-2">
                          <span class="font-mono font-medium text-foreground">
                            {{ f.name }}
                            <span v-if="f.required" class="text-rose-500">*</span>
                          </span>
                          <span class="font-mono text-[11px] text-muted-foreground">
                            {{ f.type }}
                          </span>
                        </div>
                        <div v-if="f.description" class="text-[11px] text-muted-foreground">
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

                  <div class="space-y-2">
                    <div class="flex items-center justify-between">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        请求 JSON 示例
                      </span>
                      <UiButton
                        variant="ghost"
                        size="xs"
                        @click="copyText(op.requestBody.sampleJson, '请求示例已复制')"
                      >
                        <Copy class="h-3 w-3" />
                        <span>复制</span>
                      </UiButton>
                    </div>
                    <pre
                      class="max-h-64 overflow-auto rounded-lg border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap break-all"
                    >{{ op.requestBody.sampleJson }}</pre>
                  </div>
                </div>

                <!-- Responses + cURL Example -->
                <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
                  <div class="space-y-2">
                    <div class="text-xs font-semibold tracking-wide text-muted-foreground">
                      响应状态
                    </div>
                    <div class="divide-y divide-border rounded-lg border border-border bg-card">
                      <div
                        v-for="r in op.responses"
                        :key="r.code"
                        class="flex items-start justify-between gap-3 px-3 py-2"
                      >
                        <div class="flex items-center gap-2">
                          <span
                            class="inline-flex rounded border px-1.5 py-0.2 font-mono text-[11px] font-semibold tabular-nums"
                            :class="httpCodeBadge(r.code)"
                          >
                            {{ r.code }}
                          </span>
                          <span class="text-foreground">{{ r.description }}</span>
                        </div>
                        <span
                          v-if="r.schemaLabel"
                          class="shrink-0 font-mono text-[11px] text-muted-foreground"
                        >
                          {{ r.schemaLabel }}
                        </span>
                      </div>
                    </div>
                  </div>

                  <div class="space-y-2">
                    <div class="flex items-center justify-between">
                      <span class="text-xs font-semibold tracking-wide text-muted-foreground">
                        cURL 调用示例
                      </span>
                      <UiButton
                        variant="ghost"
                        size="xs"
                        @click="copyText(op.curlSnippet, 'cURL 命令已复制')"
                      >
                        <Copy class="h-3 w-3" />
                        <span>复制 cURL</span>
                      </UiButton>
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
import { toast } from "../modules/toast";
import { openDraftInNewTab } from "../stores/workspace";
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
    title: "环境与密钥池",
    subtitle: "全局/命名环境变量、Mazbri 多租户加权密钥池及非对称密钥对生成。",
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
const expandedIds = reactive(new Set<string>());

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
      const contentMap = op.requestBody?.content;
      if (contentMap) {
        const ct =
          Object.keys(contentMap).find((k) => k.includes("json")) ||
          Object.keys(contentMap)[0];
        if (ct && contentMap[ct]) {
          const rawSchema = contentMap[ct]?.schema;
          const sample = buildSampleValue(rawSchema, schemas);
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
        const compactJson = requestBody.sampleJson.replace(/\s+/g, " ");
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

function focusOperation(opId: string): void {
  expandedIds.add(opId);
  scrollToAnchor(opId);
}

function openInStudio(op: ParsedOperation): void {
  const draft = createDefaultRequestDraft("http");
  draft.name = op.summary || `${op.method} ${op.path}`;
  draft.method = op.method;
  draft.endpoint = `${originUrl.value}${op.path}`;
  draft.params = op.parameters
    .filter((p) => p.in === "query")
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

async function copyText(text: string, msg: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(msg);
  } catch {
    toast.error("复制失败");
  }
}

function downloadOpenApiJson(): void {
  if (!spec.value) return;
  const blob = new Blob([JSON.stringify(spec.value, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "x-loadgen-openapi.json";
  a.click();
  URL.revokeObjectURL(url);
  toast.success("已下载 openapi.json");
}

async function fetchSpec(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    const { data } = await http.get<RawOpenApiDoc>("/api/v1/openapi.json");
    spec.value = data;
    expandedIds.clear();
    // Expand the first two operations so the page immediately showcases rich Shadcn schema details
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
