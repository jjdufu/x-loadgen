<template>
  <UiDialog
    :open="curlModalOpen"
    title="从 cURL 导入请求"
    description="粘贴浏览器开发者工具或终端中的 curl 命令，自动解析方法、URL、查询参数、Headers、鉴权与请求体。"
    max-width="xl"
    @close="curlModalOpen = false"
  >
    <UiTextarea
      v-model="curlText"
      :rows="8"
      mono
      placeholder="curl -X POST 'http://127.0.0.1:8757/api/v1/probe' -H 'Content-Type: application/json' -d '{&quot;hello&quot;:&quot;world&quot;}'"
    />

    <template #footer>
      <UiButton variant="outline" @click="curlModalOpen = false">取消</UiButton>
      <UiButton variant="default" @click="applyCurl">
        导入
      </UiButton>
    </template>
  </UiDialog>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { parseCurlToDraft } from "../../helpers/curl";
import { toast } from "../../modules/toast";
import { activeTab, curlModalOpen } from "../../stores/workspace";
import UiButton from "../ui/UiButton.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiTextarea from "../ui/UiTextarea.vue";

const curlText = ref("");

function applyCurl(): void {
  if (!curlText.value.trim()) {
    toast.error("请先粘贴 cURL 命令");
    return;
  }
  const parsed = parseCurlToDraft(curlText.value);
  parsed.id = activeTab.value.draft.id;
  parsed.collection_id = activeTab.value.draft.collection_id;
  activeTab.value.draft = parsed;
  curlModalOpen.value = false;
  curlText.value = "";
  toast.success("已从 cURL 导入到当前请求标签页");
}
</script>
