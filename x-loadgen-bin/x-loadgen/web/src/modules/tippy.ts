import type { App, DirectiveBinding, VNode } from "vue";
import VueTippy, { roundArrow, setDefaultProps, useTippy } from "vue-tippy";

import "tippy.js/dist/tippy.css";
import "tippy.js/animations/scale-subtle.css";
import "tippy.js/dist/border.css";
import "tippy.js/dist/svg-arrow.css";

interface TippyElement extends HTMLElement {
  $tippy?: {
    props: Record<string, unknown>;
    setProps: (opts: Record<string, unknown>) => void;
    destroy: () => void;
  };
  _tippy?: {
    props: Record<string, unknown>;
    setProps: (opts: Record<string, unknown>) => void;
    destroy: () => void;
  };
}

function resolveOpts(
  el: TippyElement,
  binding: DirectiveBinding,
  vnode: VNode,
): Record<string, unknown> {
  const opts: Record<string, unknown> =
    typeof binding.value === "string"
      ? { content: binding.value }
      : { ...(binding.value || {}) };

  if (!opts.content) {
    const title =
      (vnode.props?.title as string | undefined) ?? el.getAttribute("title");
    if (title) {
      opts.content = title;
    }
  }

  if (!opts.content && el.getAttribute("content")) {
    opts.content = el.getAttribute("content");
  }

  return opts;
}

export function registerHoppscotchTippy(app: App): void {
  app.use(VueTippy, { directive: "tippy-original" });

  app.directive("tippy", {
    mounted(el: TippyElement, binding: DirectiveBinding, vnode: VNode) {
      const opts = resolveOpts(el, binding, vnode);
      el.removeAttribute("title");
      useTippy(el, opts);
    },
    unmounted(el: TippyElement) {
      if (el._tippy) {
        el._tippy.destroy();
      } else if (el.$tippy) {
        el.$tippy.destroy();
      }
    },
    updated(el: TippyElement, binding: DirectiveBinding, vnode: VNode) {
      const tippy = el.$tippy || el._tippy;
      if (!tippy) return;
      const opts = resolveOpts(el, binding, vnode);
      if (!opts.content) {
        opts.content = "";
      }
      el.removeAttribute("title");
      if (opts.content === tippy.props?.content) return;
      tippy.setProps(opts);
    },
  });

  setDefaultProps({
    animation: "scale-subtle",
    appendTo: () => document.body,
    allowHTML: false,
    animateFill: false,
    arrow: roundArrow + roundArrow,
    popperOptions: {
      modifiers: [
        {
          name: "preventOverflow",
          options: {
            rootBoundary: "document",
          },
        },
      ],
    },
  });
}
