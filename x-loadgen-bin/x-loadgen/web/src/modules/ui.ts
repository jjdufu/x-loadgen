import type { App } from "vue";
import { plugin as HoppUI } from "@hoppscotch/ui";
import "@hoppscotch/ui/style.css";

const UI_MESSAGES: Record<string, string> = {
  "action.cancel": "取消",
  "action.confirm": "确认",
  "action.close": "关闭",
  "action.copy": "复制",
  "action.delete": "删除",
  "action.edit": "编辑",
  "action.save": "保存",
  "action.more": "更多",
  "action.turn_on": "已开启",
  "action.turn_off": "已关闭",
  "state.none": "无",
  "state.loading": "加载中...",
  "empty.default": "暂无内容",
  "modal.close": "关闭窗口",
  "tab.close": "关闭标签页",
  "tab.close_others": "关闭其他标签页",
  "tab.duplicate": "复制标签页",
};

export function translateHoppKey(key: string): string {
  if (UI_MESSAGES[key]) {
    return UI_MESSAGES[key];
  }
  const tail = key.split(".").pop() ?? key;
  return tail.replace(/_/g, " ");
}

export function registerHoppscotchUI(app: App): void {
  // @hoppscotch/ui plugin expects { t, onModalOpen, onModalClose }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  app.use(HoppUI as any, {
    t: translateHoppKey,
    onModalOpen: () => {},
    onModalClose: () => {},
  });
}
