import {
  createRouter,
  createWebHistory,
  type RouteRecordRaw,
} from "vue-router";
import { getCachedMe } from "../modules/auth";

const routes: RouteRecordRaw[] = [
  {
    path: "/login",
    name: "login",
    component: () => import("../views/LoginView.vue"),
    meta: { public: true, title: "登录" },
  },
  {
    path: "/password",
    name: "password",
    component: () => import("../views/ChangePasswordView.vue"),
    meta: { title: "修改口令" },
  },
  {
    path: "/",
    component: () => import("../components/AppLayout.vue"),
    children: [
      {
        path: "",
        name: "workspace",
        component: () => import("../views/WorkspaceView.vue"),
        meta: { title: "工作台" },
      },
      {
        path: "runs",
        name: "runs",
        component: () => import("../views/RunsView.vue"),
        meta: { title: "压测报告" },
      },
      {
        path: "runs/compare",
        name: "run-compare",
        component: () => import("../views/RunCompareView.vue"),
        meta: { title: "报告对比" },
      },
      {
        path: "runs/:id",
        name: "run-detail",
        component: () => import("../views/RunDetailView.vue"),
        meta: { title: "压测报告" },
      },
      {
        path: "runs/:id/report",
        name: "run-report",
        redirect: (to) => ({ name: "run-detail", params: { id: to.params.id } }),
      },
      {
        path: "history",
        name: "history",
        component: () => import("../views/HistoryView.vue"),
        meta: { title: "历史记录" },
      },
      {
        path: "profiles",
        name: "profiles",
        component: () => import("../views/ProfilesView.vue"),
        meta: { title: "负载预设" },
      },
      {
        path: "docs",
        name: "docs",
        component: () => import("../views/ApiDocsView.vue"),
        meta: { title: "接口文档" },
      },
      {
        path: "settings",
        name: "settings",
        component: () => import("../views/SettingsView.vue"),
        meta: { title: "系统设置" },
      },
    ],
  },
  {
    path: "/:pathMatch(.*)*",
    redirect: "/",
  },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

router.beforeEach(async (to) => {
  try {
    const data = await getCachedMe();
    if (!data.auth_required && to.name === "login") {
      return { name: "workspace" };
    }
    if (data.must_change_password && to.name !== "password") {
      return { name: "password" };
    }
    if (!data.must_change_password && to.name === "password") {
      return { name: "settings" };
    }
    if (to.name === "login") {
      return { name: "workspace" };
    }
    return true;
  } catch {
    if (to.meta.public || to.name === "login") {
      return true;
    }
    return { name: "login" };
  }
});

router.afterEach((to) => {
  const pageTitle = typeof to.meta.title === "string" ? to.meta.title : "工作台";
  document.title = `${pageTitle} · x-loadgen`;
});

export default router;
