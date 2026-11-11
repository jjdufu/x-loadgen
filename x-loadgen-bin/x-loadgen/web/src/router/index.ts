import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import { http } from "../api/http";
import type { MeResponse } from "../api/types";

const routes: RouteRecordRaw[] = [
  { path: "/login", name: "login", component: () => import("../views/LoginView.vue"), meta: { public: true } },
  { path: "/password", name: "password", component: () => import("../views/ChangePasswordView.vue") },
  {
    path: "/",
    component: () => import("../components/AppLayout.vue"),
    children: [
      { path: "", redirect: "/runs" },
      { path: "targets", name: "targets", component: () => import("../views/TargetsView.vue") },
      { path: "targets/new", name: "target-new", component: () => import("../views/TargetDetailView.vue") },
      { path: "targets/:id", name: "target-detail", component: () => import("../views/TargetDetailView.vue") },
      { path: "templates", name: "templates", component: () => import("../views/TemplatesView.vue") },
      { path: "templates/new", name: "template-new", component: () => import("../views/TemplateDetailView.vue") },
      { path: "templates/:id", name: "template-detail", component: () => import("../views/TemplateDetailView.vue") },
      { path: "runs", name: "runs", component: () => import("../views/RunsView.vue") },
      { path: "runs/:id", name: "run-detail", component: () => import("../views/RunDetailView.vue") },
      { path: "runs/:id/report", name: "run-report", component: () => import("../views/ReportView.vue") },
      { path: "settings", name: "settings", component: () => import("../views/SettingsView.vue") },
    ],
  },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

router.beforeEach(async (to) => {
  try {
    const { data } = await http.get<MeResponse>("/api/v1/auth/me");
    if (!data.auth_required && to.name === "login") {
      return { name: "runs" };
    }
    if (data.must_change_password && to.name !== "password") {
      return { name: "password" };
    }
    if (!data.must_change_password && to.name === "password") {
      return { name: "settings" };
    }
    if (to.name === "login") {
      return { name: "runs" };
    }
    return true;
  } catch {
    if (to.meta.public || to.name === "login") {
      return true;
    }
    return { name: "login" };
  }
});

export default router;
