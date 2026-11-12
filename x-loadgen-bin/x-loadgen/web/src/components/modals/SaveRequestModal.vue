<template>
  <UiDialog
    :open="saveModalOpen"
    :title="saveModalForceNew ? '另存请求' : '保存请求'"
    description="归档后的请求可随时复用，或与同集合内其他接口组合执行加权压测。"
    max-width="lg"
    @close="saveModalOpen = false"
  >
    <div class="space-y-4">
      <div class="space-y-1.5">
        <div class="flex h-5 items-center justify-between gap-2">
          <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
            请求名称 *
          </label>
        </div>
        <UiInput
          v-model="requestName"
          placeholder="例如 查询订单详情"
        />
      </div>

      <div class="space-y-1.5">
        <div class="flex h-5 items-center justify-between gap-2">
          <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
            所属集合 *
          </label>
          <button
            v-if="folderOptions.length > 0"
            type="button"
            class="inline-flex h-5 cursor-pointer items-center gap-1 rounded px-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            @click="creatingFolder = !creatingFolder"
          >
            <FolderPlus class="h-3.5 w-3.5" />
            <span>{{ creatingFolder ? "选择已有集合" : "新建集合" }}</span>
          </button>
        </div>

        <UiInput
          v-if="creatingFolder || folderOptions.length === 0"
          v-model="newFolderName"
          placeholder="输入新集合名称，例如 核心业务网关压测集"
        />
        <UiSelect v-else v-model="selectedCollectionId" mono>
          <option v-for="opt in folderOptions" :key="opt.id" :value="opt.id">
            {{ opt.label }}
          </option>
        </UiSelect>
      </div>

      <div class="space-y-1.5">
        <div class="flex h-5 items-center justify-between gap-2">
          <label class="text-xs font-medium text-muted-foreground whitespace-nowrap">
            默认压测权重
          </label>
        </div>
        <UiInput
          :model-value="weight"
          type="number"
          min="1"
          mono
          class="w-36"
          @update:model-value="(v) => (weight = Math.max(1, Number(v) || 1))"
        />
      </div>
    </div>

    <template #footer>
      <UiButton variant="outline" @click="saveModalOpen = false">取消</UiButton>
      <UiButton variant="default" :loading="saving" @click="confirmSave">
        保存
      </UiButton>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
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
import UiButton from "../ui/UiButton.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiInput from "../ui/UiInput.vue";
import UiSelect from "../ui/UiSelect.vue";

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
  requestName.value = draft.name || "新建请求";
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
    const snap = JSON.stringify(normalizeDraftForApi(activeTab.value.draft));
    activeTab.value.savedSnapshotJson = snap;
    activeTab.value.baselineSnapshotJson = snap;
    await refreshCollections();
    toast.success("请求已保存至集合");
    saveModalOpen.value = false;
  } finally {
    saving.value = false;
  }
}
</script>
