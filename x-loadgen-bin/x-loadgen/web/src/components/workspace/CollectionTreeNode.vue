<template>
  <div class="select-none">
    <!-- Collection / Folder Header Row -->
    <div
      class="flex items-center justify-between px-2 py-1.5 hover:bg-primaryLight rounded group cursor-pointer"
      :style="{ paddingLeft: `${depth * 14 + 8}px` }"
      @click="expanded = !expanded"
    >
      <div class="flex items-center space-x-1.5 min-w-0 flex-1">
        <component
          :is="expanded ? ChevronDown : ChevronRight"
          class="w-3.5 h-3.5 text-secondaryLight flex-shrink-0"
        />
        <Folder class="w-3.5 h-3.5 text-accent flex-shrink-0" />
        <span class="truncate text-secondaryDark font-semibold text-tiny">
          {{ node.name }}
        </span>
        <span class="text-[10px] text-secondaryLight font-mono">
          ({{ totalCount }})
        </span>
      </div>

      <div
        class="flex items-center space-x-0.5 opacity-0 group-hover:opacity-100 transition flex-shrink-0"
        @click.stop
      >
        <button
          v-tippy="'按权重压测整个集合'"
          type="button"
          class="p-1 rounded hover:bg-primaryDark text-accent"
          @click="openCollectionLoadRunner(node.id, node.name)"
        >
          <Play class="w-3.5 h-3.5" />
        </button>
        <button
          v-tippy="'在此集合新建请求'"
          type="button"
          class="p-1 rounded hover:bg-primaryDark text-secondary hover:text-secondaryDark"
          @click="emit('create-request', node)"
        >
          <FilePlus2 class="w-3.5 h-3.5" />
        </button>
        <button
          v-tippy="'新建子文件夹'"
          type="button"
          class="p-1 rounded hover:bg-primaryDark text-secondary hover:text-secondaryDark"
          @click="emit('create-subfolder', node)"
        >
          <FolderPlus class="w-3.5 h-3.5" />
        </button>
        <button
          v-tippy="'删除集合'"
          type="button"
          class="p-1 rounded hover:bg-primaryDark text-rose-400"
          @click="emit('delete-collection', node)"
        >
          <Trash2 class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- Children: Subfolders + Saved Requests -->
    <div v-if="expanded">
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
        class="flex items-center justify-between px-2 py-1.5 hover:bg-primaryLight rounded group cursor-pointer text-tiny"
        :class="{
          'bg-primaryDark text-secondaryDark': activeTab.draft.id === req.id,
        }"
        :style="{ paddingLeft: `${(depth + 1) * 14 + 14}px` }"
        @click="openSavedRequestInTab(req)"
      >
        <div class="flex items-center space-x-2 min-w-0 flex-1">
          <span
            class="font-mono font-bold text-[10px] w-10 flex-shrink-0"
            :class="methodColorClass(req.method)"
          >
            {{ req.method }}
          </span>
          <span
            v-if="req.protocol === 'mazbri'"
            class="px-1 rounded bg-accent/15 text-accent font-mono text-[9px] flex-shrink-0"
          >
            MZ
          </span>
          <span class="truncate text-secondaryDark">{{ req.name }}</span>
        </div>

        <div
          class="flex items-center space-x-0.5 opacity-0 group-hover:opacity-100 transition flex-shrink-0"
          @click.stop
        >
          <button
            v-tippy="'直接压测此请求'"
            type="button"
            class="p-1 rounded hover:bg-primaryDark text-accent"
            @click="openSingleLoadRunner(req)"
          >
            <Zap class="w-3 h-3" />
          </button>
          <button
            v-tippy="'删除请求'"
            type="button"
            class="p-1 rounded hover:bg-primaryDark text-rose-400"
            @click="emit('delete-request', req)"
          >
            <Trash2 class="w-3 h-3" />
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
