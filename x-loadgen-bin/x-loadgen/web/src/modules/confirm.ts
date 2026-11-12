import { ref } from "vue";

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmText?: string;
  cancelText?: string;
  variant?: "default" | "destructive";
}

interface PendingConfirm extends Required<ConfirmOptions> {
  resolve: (confirmed: boolean) => void;
}

export const activeConfirm = ref<PendingConfirm | null>(null);

export function askConfirm(options: ConfirmOptions): Promise<boolean> {
  if (activeConfirm.value) {
    activeConfirm.value.resolve(false);
  }
  return new Promise<boolean>((resolve) => {
    activeConfirm.value = {
      title: options.title,
      description: options.description ?? "",
      confirmText: options.confirmText ?? "确认",
      cancelText: options.cancelText ?? "取消",
      variant: options.variant ?? "destructive",
      resolve,
    };
  });
}

export function resolveConfirm(confirmed: boolean): void {
  const current = activeConfirm.value;
  activeConfirm.value = null;
  if (current) {
    current.resolve(confirmed);
  }
}
