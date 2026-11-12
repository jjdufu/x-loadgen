<template>
  <div class="select-none">
    <!-- Collection / Folder Header Row -->
    <div
      class="group flex cursor-pointer items-center justify-between rounded-md px-2 py-1.5 transition-colors hover:bg-accent/60"
      :style="{ paddingLeft: `${depth * 14 + 8}px` }"
      @click="expanded = !expanded"
    >
      <div class="flex min-w-0 flex-1 items-center gap-1.5">
        <component
          :is="expanded ? ChevronDown : ChevronRight"
          class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
        />
        <Folder class="h-3.5 w-3.5 shrink-0 text-foreground/80" />
        <span class="truncate text-xs font-medium text-foreground">
          {{ node.name }}
        </span>
        <span class="font-mono text-[10px] text-muted-foreground tabular-nums">
          ({{ totalCount }})
        </span>
      </div>

      <div
        class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
        @click.stop
      >
        <button
          type="button"
          title="按权重压测整个集合"
          aria-label="压测集合"
          class="rounded p-1 text-foreground hover:bg-background"
          @click="openCollectionLoadRunner(node.id, node.name)"
        >
          <Play class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title="在此集合新建请求"
          aria-label="在此集合新建请求"
          class="rounded p-1 text-muted-foreground hover:bg-background hover:text-foreground"
          @click="emit('create-request', node)"
        >
          <FilePlus2 class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title="新建子文件夹"
          aria-label="新建子文件夹"
          class="rounded p-1 text-muted-foreground hover:bg-background hover:text-foreground"
          @click="emit('create-subfolder', node)"
        >
          <FolderPlus class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title="删除集合"
          aria-label="删除集合"
          class="rounded p-1 text-muted-foreground hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
          @click="emit('delete-collection', node)"
        >
          <Trash2 class="h-3.5 w-3.5" />
        </button>
      </div>
    </div>

    <!-- Children: Subfolders + Saved Requests -->
    <div v-if="expanded" class="mt-0.5 space-y-0.5">
      <CollectionTreeNode
        v-for="sub in node.folders"
        :key="sub.id"
        :node="sub"
        :depth="depth + 1"
        @create-request="emit('create-request', $event)"
        @create-subfolder="emit('create-subfolder', $event)"
        @delete-collection="emit('delete-collection', $event)"
        @delete-request="emit('delete-request', $event)"
      />

      <div
        v-for="req in node.requests"
        :key="req.id"
        class="group flex cursor-pointer items-center justify-between rounded-md px-2 py-1.5 text-xs transition-colors"
        :class="
          activeTab.draft.id === req.id
            ? 'bg-accent text-accent-foreground font-medium'
            : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'
        "
        :style="{ paddingLeft: `${(depth + 1) * 14 + 14}px` }"
        @click="openSavedRequestInTab(req)"
      >
        <div class="flex min-w-0 flex-1 items-center gap-2">
          <span
            class="w-10 shrink-0 font-mono text-[10px] font-semibold"
            :class="methodColorClass(req.method)"
          >
            {{ req.method }}
          </span>
          <span
            v-if="req.protocol === 'mazbri'"
            class="shrink-0 rounded border border-border bg-muted px-1 font-mono text-[9px] text-foreground"
          >
            MZ
          </span>
          <span class="truncate">{{ req.name }}</span>
        </div>

        <div
          class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
          @click.stop
        >
          <button
            type="button"
            title="直接压测此请求"
            aria-label="压测此请求"
            class="rounded p-1 text-foreground hover:bg-background"
            @click="openSingleLoadRunner(req)"
          >
            <Zap class="h-3 w-3" />
          </button>
          <button
            type="button"
            title="删除请求"
            aria-label="删除请求"
            class="rounded p-1 text-muted-foreground hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
            @click="emit('delete-request', req)"
          >
            <Trash2 class="h-3 w-3" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  ChevronDown,
  ChevronRight,
  FilePlus2,
  Folder,
  FolderPlus,
  Play,
  Trash2,
  Zap,
} from "lucide-vue-next";
import type { CollectionNode, SavedRequest } from "../../api/types";
import { methodColorClass } from "../../helpers/format";
import {
  activeTab,
  openCollectionLoadRunner,
  openSavedRequestInTab,
  openSingleLoadRunner,
} from "../../stores/workspace";

defineOptions({
  name: "CollectionTreeNode",
});

const props = withDefaults(
  defineProps<{
    node: CollectionNode;
    depth?: number;
  }>(),
  {
    depth: 0,
  },
);

const emit = defineEmits<{
  "create-request": [node: CollectionNode];
  "create-subfolder": [node: CollectionNode];
  "delete-collection": [node: CollectionNode];
  "delete-request": [req: SavedRequest];
}>();

const expanded = ref(true);

const totalCount = computed(
  () => props.node.requests.length + props.node.folders.length,
);
</script>
