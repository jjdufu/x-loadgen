import {
  createRouter,
  createWebHistory,
  type RouteRecordRaw,
} from "vue-router";
import { http } from "../api/http";
import type { MeResponse } from "../api/types";

const routes: RouteRecordRaw[] = [
  {
    path: "/login",
    name: "login",
    component: () => import("../views/LoginView.vue"),
    meta: { public: true },
  },
  {
    path: "/password",
    name: "password",
    component: () => import("../views/ChangePasswordView.vue"),
  },
  {
    path: "/",
    component: () => import("../components/AppLayout.vue"),
    children: [
      {
        path: "",
        name: "workspace",
        component: () => import("../views/WorkspaceView.vue"),
      },
      {
        path: "runs",
        name: "runs",
        component: () => import("../views/RunsView.vue"),
      },
      {
        path: "runs/:id",
        name: "run-detail",
        component: () => import("../views/RunDetailView.vue"),
      },
      {
        path: "runs/:id/report",
        name: "run-report",
        redirect: (to) => ({ name: "run-detail", params: { id: to.params.id } }),
      },
      {
        path: "profiles",
        name: "profiles",
        component: () => import("../views/ProfilesView.vue"),
      },
      {
        path: "docs",
        name: "docs",
        component: () => import("../views/ApiDocsView.vue"),
      },
      {
        path: "settings",
        name: "settings",
        component: () => import("../views/SettingsView.vue"),
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
    const { data } = await http.get<MeResponse>("/api/v1/auth/me");
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

export default router;
