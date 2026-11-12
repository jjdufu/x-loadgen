/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<object, object, unknown>;
  export default component;
}

declare module "splitpanes" {
  import type { DefineComponent } from "vue";
  export const Splitpanes: DefineComponent<
    {
      horizontal?: boolean;
      dblClickSplitter?: boolean;
      pushOtherPanes?: boolean;
      firstSplitter?: boolean;
      maximizePanes?: boolean;
      rtl?: boolean;
      class?: string;
    },
    object,
    unknown
  >;
  export const Pane: DefineComponent<
    {
      size?: number | string;
      minSize?: number | string;
      maxSize?: number | string;
      class?: string;
    },
    object,
    unknown
  >;
}
