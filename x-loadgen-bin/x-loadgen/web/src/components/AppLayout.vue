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
import {
  matchShortcut,
  recordingShortcutId,
  shortcutBindings,
} from "../modules/shortcuts";
import {
  activeTab,
  addNewRequestTab,
  bootstrapWorkspace,
  refreshMeta,
  responsePaneOpen,
  saveActiveTab,
  sendActiveTabProbe,
  switchActiveTabProtocol,
} from "../stores/workspace";

const route = useRoute();
let metaTimer: number | null = null;

function handleGlobalKeydown(e: KeyboardEvent): void {
  if (recordingShortcutId.value !== null) return;
  if (document.querySelector('[role="dialog"]')) return;
  if (route.name !== "workspace") return;

  const bindings = shortcutBindings.value;
  if (matchShortcut(e, bindings.sendRequest)) {
    e.preventDefault();
    void sendActiveTabProbe();
  } else if (matchShortcut(e, bindings.saveRequest)) {
    e.preventDefault();
    void saveActiveTab();
  } else if (matchShortcut(e, bindings.newRequest)) {
    e.preventDefault();
    addNewRequestTab(
      activeTab.value.draft.protocol === "mazbri" ? "mazbri" : "http",
    );
  } else if (matchShortcut(e, bindings.switchProtocol)) {
    e.preventDefault();
    switchActiveTabProtocol(
      activeTab.value.draft.protocol === "http" ? "mazbri" : "http",
    );
  } else if (matchShortcut(e, bindings.toggleResponse)) {
    e.preventDefault();
    responsePaneOpen.value = !responsePaneOpen.value;
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
