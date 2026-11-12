<template>
  <HoppSmartModal
    v-if="curlModalOpen"
    dialog
    title="从 cURL 导入请求 (Import cURL)"
    @close="curlModalOpen = false"
  >
    <template #body>
      <div class="space-y-3">
        <p class="text-tiny text-secondaryLight">
          粘贴浏览器开发者工具或终端中的 <code class="font-mono text-accent">curl</code> 命令，自动解析方法、URL、查询参数、Headers、Bearer/Basic 鉴权与请求体。
        </p>
        <textarea
          v-model="curlText"
          rows="8"
          placeholder="curl -X POST 'https://echo.hoppscotch.io' -H 'Content-Type: application/json' -d '{&quot;hello&quot;:&quot;world&quot;}'"
          class="textarea font-mono text-tiny bg-primaryLight"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-end space-x-2 w-full">
        <HoppButtonSecondary label="取消" outline @click="curlModalOpen = false" />
        <HoppButtonPrimary label="解析并填入当前标签页" @click="applyCurl" />
      </div>
    </template>
  </HoppSmartModal>
</template>

<script setup lang="ts">
import { ref } from "vue";
import {
  HoppButtonPrimary,
  HoppButtonSecondary,
  HoppSmartModal,
} from "@hoppscotch/ui";
import { parseCurlToDraft } from "../../helpers/curl";
import { toast } from "../../modules/toast";
import { activeTab, curlModalOpen } from "../../stores/workspace";

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
