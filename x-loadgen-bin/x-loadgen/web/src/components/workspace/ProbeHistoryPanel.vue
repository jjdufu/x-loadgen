<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
    <div class="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
      <span class="text-xs font-semibold tracking-tight text-foreground">单发历史</span>
      <UiButton
        variant="ghost"
        size="icon-xs"
        title="刷新"
        aria-label="刷新单发历史"
        @click="reload"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="loading ? 'animate-spin' : ''" />
      </UiButton>
    </div>

    <div class="border-b border-border p-2.5">
      <UiSearchInput
        v-model="searchText"
        placeholder="按名称、URL、request_id、trace_id 筛选..."
      />
    </div>

    <div class="flex-1 overflow-y-auto p-2">
      <div
        v-if="filtered.length === 0"
        class="flex flex-col items-center justify-center gap-3 px-4 py-12 text-center text-muted-foreground"
      >
        <History class="h-8 w-8 opacity-35" />
        <p class="text-xs leading-relaxed">
          {{ searchText.trim() ? "无匹配的单发记录" : "暂无单发记录，点击「发送」后会出现在这里" }}
        </p>
      </div>

      <div v-for="group in grouped" :key="group.day" class="mb-2">
        <div class="px-1.5 py-1 text-[11px] font-medium text-muted-foreground">
          {{ group.day }}
        </div>
        <div
          v-for="item in group.items"
          :key="item.id"
          class="mb-1 rounded-md border transition-colors"
          :class="
            expandedId === item.id
              ? 'border-border bg-accent/30'
              : 'border-transparent hover:bg-accent/40'
          "
        >
          <button
            type="button"
            class="flex w-full cursor-pointer items-center gap-2 px-2 py-1.5 text-left"
            :aria-expanded="expandedId === item.id"
            @click="toggle(item)"
          >
            <span
              class="w-12 shrink-0 font-mono text-[10px] font-semibold"
              :class="methodColorClass(item.method)"
            >
              {{ item.method }}
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-xs text-foreground">{{ item.request_name }}</span>
              <span class="block truncate font-mono text-[10px] text-muted-foreground">
                {{ item.url }}
              </span>
            </span>
            <span class="shrink-0 text-right">
              <span
                class="block rounded border px-1 font-mono text-[10px] tabular-nums"
                :class="statusCategoryClass(item.http_status, item.transport_ok)"
              >
                {{ item.http_status ?? item.error_class ?? "ERR" }}
              </span>
              <span class="block font-mono text-[10px] text-muted-foreground tabular-nums">
                {{ timeOf(item.created_at) }}
              </span>
            </span>
          </button>

          <div
            v-if="expandedId === item.id"
            class="space-y-1.5 border-t border-border px-2.5 py-2 font-mono text-[11px] select-text"
          >
            <div class="flex flex-wrap items-center gap-1.5">
              <UiBadge :variant="item.passed_assertions ? 'success' : 'destructive'">
                {{ item.passed_assertions ? "断言通过" : "断言未通过" }}
              </UiBadge>
              <UiBadge :variant="traceEchoVariant(item.trace_echo)">
                {{ traceEchoLabel(item.trace_echo) }}
              </UiBadge>
              <span class="text-muted-foreground">
                {{ formatLatencyUs(item.latency_us) }} · {{ formatBytes(item.response_bytes) }}
              </span>
            </div>
            <button
              type="button"
              class="block w-full truncate text-left text-muted-foreground hover:text-foreground"
              title="复制 X-Request-Id"
              @click="copy(item.request_id, 'X-Request-Id')"
            >
              X-Request-Id <span class="text-foreground">{{ item.request_id }}</span>
            </button>
            <button
              type="button"
              class="block w-full truncate text-left text-muted-foreground hover:text-foreground"
              title="复制 trace_id"
              @click="copy(item.trace_id, 'trace_id')"
            >
              trace_id <span class="text-foreground">{{ item.trace_id }}</span>
            </button>
            <div v-if="item.environment_name || item.tenant_name" class="text-muted-foreground">
              <span v-if="item.environment_name">环境 {{ item.environment_name }}</span>
              <span v-if="item.environment_name && item.tenant_name"> · </span>
              <span v-if="item.tenant_name">租户 {{ item.tenant_name }}</span>
            </div>
            <div v-if="item.error" class="break-all text-rose-600 dark:text-rose-400">
              {{ item.error }}
            </div>
            <div
              v-for="(value, name) in item.captures"
              :key="name"
              class="break-all text-muted-foreground"
            >
              {{ name }} = <span class="text-foreground">{{ value }}</span>
            </div>
            <pre
              v-if="detail && detail.id === item.id"
              class="max-h-60 overflow-auto whitespace-pre-wrap break-all rounded border border-border bg-muted/40 p-2 text-[11px] text-foreground"
            >{{ detail.response_body || "（空响应体）" }}{{ detail.response_truncated ? "\n…（已截断，最多保存 64KB）" : "" }}</pre>
            <div class="flex items-center justify-end gap-1 pt-1">
              <UiButton
                v-if="item.saved_request_id"
                variant="ghost"
                size="xs"
                @click="openSaved(item.saved_request_id)"
              >
                <ExternalLink class="h-3 w-3" />
                <span>打开请求</span>
              </UiButton>
              <UiButton variant="ghost" size="xs" @click="remove(item)">
                <Trash2 class="h-3 w-3" />
                <span>删除</span>
              </UiButton>
            </div>
          </div>
        </div>
      </div>

      <div v-if="nextBeforeId !== null" class="flex justify-center py-2">
        <UiButton variant="outline" size="xs" :disabled="loading" @click="loadMore">
          加载更多
        </UiButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { ExternalLink, History, RefreshCw, Trash2 } from "lucide-vue-next";
import { api } from "../../api/workspace";
import type { ProbeRecord } from "../../api/types";
import {
  formatBytes,
  formatLatencyUs,
  methodColorClass,
  statusCategoryClass,
  traceEchoLabel,
  traceEchoVariant,
} from "../../helpers/format";
import { toast } from "../../modules/toast";
import { openSavedRequestInTab, probeHistoryVersion } from "../../stores/workspace";
import { copyText } from "../../utils/clipboard";
import UiBadge from "../ui/UiBadge.vue";
import UiButton from "../ui/UiButton.vue";
import UiSearchInput from "../ui/UiSearchInput.vue";

const PAGE = 50;

const items = ref<ProbeRecord[]>([]);
const nextBeforeId = ref<number | null>(null);
const loading = ref(false);
const searchText = ref("");
const expandedId = ref<number | null>(null);
const detail = ref<ProbeRecord | null>(null);

const filtered = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  if (!q) return items.value;
  return items.value.filter((p) =>
    [p.request_name, p.url, p.request_id, p.trace_id].some((field) =>
      field.toLowerCase().includes(q),
    ),
  );
});

const grouped = computed(() => {
  const groups: { day: string; items: ProbeRecord[] }[] = [];
  for (const item of filtered.value) {
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

async function reload(): Promise<void> {
  loading.value = true;
  try {
    const page = await api.listProbes({ limit: PAGE });
    items.value = page.items;
    nextBeforeId.value = page.next_before_id;
  } finally {
    loading.value = false;
  }
}

async function loadMore(): Promise<void> {
  if (nextBeforeId.value === null) return;
  loading.value = true;
  try {
    const page = await api.listProbes({ limit: PAGE, before_id: nextBeforeId.value });
    items.value = [...items.value, ...page.items];
    nextBeforeId.value = page.next_before_id;
  } finally {
    loading.value = false;
  }
}

async function toggle(item: ProbeRecord): Promise<void> {
  if (expandedId.value === item.id) {
    expandedId.value = null;
    return;
  }
  expandedId.value = item.id;
  detail.value = null;
  detail.value = await api.getProbe(item.id);
}

async function remove(item: ProbeRecord): Promise<void> {
  await api.deleteProbe(item.id);
  items.value = items.value.filter((p) => p.id !== item.id);
  if (expandedId.value === item.id) expandedId.value = null;
  toast.success("已删除单发记录");
}

async function openSaved(id: string): Promise<void> {
  openSavedRequestInTab(await api.getRequest(id));
}

async function copy(value: string, label: string): Promise<void> {
  if (await copyText(value)) toast.success(`已复制 ${label}`);
}

watch(probeHistoryVersion, () => {
  void reload();
});
onMounted(() => {
  void reload();
});
</script>
