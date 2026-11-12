<template>
  <HoppSmartModal
    v-if="saveModalOpen"
    dialog
    :title="saveModalForceNew ? '另存请求到集合 (Save As)' : '保存请求到集合 (Save Request)'"
    @close="saveModalOpen = false"
  >
    <template #body>
      <div class="space-y-4">
        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            请求名称 (Request Name) *
          </label>
          <input
            v-model="requestName"
            type="text"
            placeholder="例如 查询订单详情"
            class="input"
          />
        </div>

        <div>
          <div class="flex items-center justify-between mb-1">
            <label class="text-tiny text-secondaryLight font-semibold">
              所属集合 / 文件夹 (Collection) *
            </label>
            <HoppButtonSecondary
              :icon="FolderPlus"
              :label="creatingFolder ? '返回选择已有集合' : '快速新建集合'"
              @click="creatingFolder = !creatingFolder"
            />
          </div>

          <div v-if="creatingFolder || folderOptions.length === 0" class="flex items-center space-x-2">
            <input
              v-model="newFolderName"
              type="text"
              placeholder="输入新集合名称，例如 核心业务网关压测集"
              class="input flex-1"
            />
          </div>
          <select v-else v-model="selectedCollectionId" class="select font-mono">
            <option v-for="opt in folderOptions" :key="opt.id" :value="opt.id">
              {{ opt.label }}
            </option>
          </select>
        </div>

        <div>
          <label class="block text-tiny text-secondaryLight mb-1 font-semibold">
            集合压测默认权重 (Weight)
          </label>
          <input
            v-model.number="weight"
            type="number"
            min="1"
            class="input font-mono w-32"
          />
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-end space-x-2 w-full">
        <HoppButtonSecondary label="取消" outline @click="saveModalOpen = false" />
        <HoppButtonPrimary :loading="saving" label="确认保存" @click="confirmSave" />
      </div>
    </template>
  </HoppSmartModal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartModal,
} from "@hoppscotch/ui";
import { FolderPlus } from "lucide-vue-next";
import { api } from "../../api/workspace";
import { cloneJson, normalizeDraftForApi } from "../../helpers/format";
import { toast } from "../../modules/toast";
import {
  activeTab,
  collections,
  flattenCollectionOptions,
  refreshCollections,
  saveModalForceNew,
  saveModalOpen,
} from "../../stores/workspace";

const requestName = ref("");
const selectedCollectionId = ref("");
const creatingFolder = ref(false);
const newFolderName = ref("");
const weight = ref(1);
const saving = ref(false);

const folderOptions = computed(() =>
  flattenCollectionOptions(collections.value),
);

watch(saveModalOpen, (open) => {
  if (!open) return;
  const draft = activeTab.value.draft;
  requestName.value = draft.name || "未命名请求";
  weight.value = Math.max(1, draft.weight || 1);
  creatingFolder.value = folderOptions.value.length === 0;
  newFolderName.value = "";
  selectedCollectionId.value =
    draft.collection_id || folderOptions.value[0]?.id || "";
});

async function confirmSave(): Promise<void> {
  if (!requestName.value.trim()) {
    toast.error("请输入请求名称");
    return;
  }
  saving.value = true;
  try {
    let targetColId = selectedCollectionId.value;
    if (creatingFolder.value || folderOptions.value.length === 0) {
      if (!newFolderName.value.trim()) {
        toast.error("请输入新集合名称");
        return;
      }
      const col = await api.createCollection({
        name: newFolderName.value.trim(),
        default_protocol: activeTab.value.draft.protocol,
      });
      targetColId = col.id;
    }

    const payload = normalizeDraftForApi({
      ...activeTab.value.draft,
      name: requestName.value.trim(),
      collection_id: targetColId,
      weight: Math.max(1, weight.value || 1),
    });

    const saved =
      !saveModalForceNew.value && payload.id
        ? await api.updateRequest(payload.id, payload)
        : await api.createRequest({ ...payload, id: null });

    activeTab.value.draft = cloneJson(saved);
    activeTab.value.savedSnapshotJson = JSON.stringify(
      normalizeDraftForApi(activeTab.value.draft),
    );
    await refreshCollections();
    toast.success(`请求已保存至集合`);
    saveModalOpen.value = false;
  } finally {
    saving.value = false;
  }
}
</script>
