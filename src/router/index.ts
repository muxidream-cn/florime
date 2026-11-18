import { createRouter, createWebHashHistory } from "vue-router"

import AppShell from "@/components/AppShell.vue"

const router = createRouter({
  // hash 模式：Tauri 的 toast 窗口用 index.html#/toast 直接进独立页
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      component: AppShell,
      children: [
        {
          path: "",
          name: "dashboard",
          component: () => import("@/views/DashboardView.vue"),
          meta: { title: "概览" },
        },
        {
          path: "rules",
          name: "rules",
          component: () => import("@/views/RulesView.vue"),
          meta: { title: "规则" },
        },
        {
          // 新建与编辑共用同一页：静态段优先于 :id，无需额外约束
          path: "rules/new",
          name: "rule-new",
          component: () => import("@/views/RuleEditorView.vue"),
          // fullBleed：编辑页的吸底保存栏要贴住内容区底边，不能吃 main 的内边距
          meta: { title: "新建规则", fullBleed: true },
        },
        {
          path: "rules/:id",
          name: "rule-edit",
          component: () => import("@/views/RuleEditorView.vue"),
          meta: { title: "编辑规则", fullBleed: true },
        },
        {
          path: "stats",
          name: "stats",
          component: () => import("@/views/StatsView.vue"),
          meta: { title: "统计" },
        },
        {
          path: "settings",
          name: "settings",
          component: () => import("@/views/SettingsView.vue"),
          meta: { title: "设置" },
        },
      ],
    },
    {
      path: "/toast",
      name: "toast",
      component: () => import("@/views/ToastView.vue"),
    },
    {
      // 休息提醒必须是独立顶层路由：不能挂在 AppShell 下，否则会带出侧边栏
      path: "/rest",
      name: "rest",
      component: () => import("@/views/RestView.vue"),
    },
  ],
})

export default router
