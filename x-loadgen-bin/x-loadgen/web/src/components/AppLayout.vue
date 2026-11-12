<template>
  <div class="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
    <AppHeader />

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <AppSidenav />
      <main class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
        <RouterView />
      </main>
    </div>

    <AppFooter />

    <!-- Global Workspace Modals -->
    <EnvironmentModal />
    <SaveRequestModal />
    <LoadRunnerModal />
    <ImportCurlModal />
    <ShortcutsModal />
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useRoute } from "vue-router";
import AppFooter from "./app/AppFooter.vue";
import AppHeader from "./app/AppHeader.vue";
import AppSidenav from "./app/AppSidenav.vue";
import EnvironmentModal from "./modals/EnvironmentModal.vue";
import ImportCurlModal from "./modals/ImportCurlModal.vue";
import LoadRunnerModal from "./modals/LoadRunnerModal.vue";
import SaveRequestModal from "./modals/SaveRequestModal.vue";
import ShortcutsModal from "./modals/ShortcutsModal.vue";
import {
  addNewRequestTab,
  bootstrapWorkspace,
  refreshMeta,
  saveActiveTab,
  sendActiveTabProbe,
  shortcutsModalOpen,
} from "../stores/workspace";

const route = useRoute();
let metaTimer: number | null = null;

function handleGlobalKeydown(e: KeyboardEvent): void {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key === "/") {
    e.preventDefault();
    shortcutsModalOpen.value = !shortcutsModalOpen.value;
    return;
  }
  if (route.name !== "workspace") return;

  if (mod && e.key === "Enter") {
    e.preventDefault();
    void sendActiveTabProbe();
  } else if (mod && e.key.toLowerCase() === "s") {
    e.preventDefault();
    void saveActiveTab();
  } else if (e.altKey && e.key.toLowerCase() === "n") {
    e.preventDefault();
    addNewRequestTab("http");
  } else if (e.altKey && e.key.toLowerCase() === "m") {
    e.preventDefault();
    addNewRequestTab("mazbri");
  }
}

onMounted(async () => {
  window.addEventListener("keydown", handleGlobalKeydown);
  await bootstrapWorkspace();
  metaTimer = window.setInterval(() => {
    void refreshMeta();
  }, 4000);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalKeydown);
  if (metaTimer !== null) {
    window.clearInterval(metaTimer);
    metaTimer = null;
  }
});
</script>
