<script setup lang="ts">
import { ChartColumn, ChevronRight, Gauge, ListChecks, PanelLeft, Settings as SettingsIcon } from "@lucide/vue"
import { reactive, watch, type Component } from "vue"
import { useRoute, useRouter, type RouteLocationRaw } from "vue-router"

import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  SidebarRail,
  useSidebar,
} from "@/components/ui/sidebar"
import { settingsSections } from "@/lib/settingsSections"

interface NavChild {
  title: string
  /** 目标路由名 */
  name: string
  /** 设置类子项携带的 ?section= 参数 */
  section?: string
}

interface NavEntry {
  /** 路由名，同时用作折叠状态的 key */
  name: string
  label: string
  icon: Component
  children?: NavChild[]
}

const route = useRoute()
const router = useRouter()
const { state, toggleSidebar } = useSidebar()

const nav: NavEntry[] = [
  { name: "dashboard", label: "概览", icon: Gauge },
  {
    name: "rules",
    label: "规则",
    icon: ListChecks,
    children: [
      { title: "规则列表", name: "rules" },
      { title: "新建规则", name: "rule-new" },
    ],
  },
  { name: "stats", label: "统计", icon: ChartColumn },
  {
    name: "settings",
    label: "设置",
    icon: SettingsIcon,
    // 二级菜单由设置页的分区清单驱动，和页面锚点保持同一来源
    children: settingsSections.map((s) => ({ title: s.title, name: "settings", section: s.id })),
  },
]

/**
 * 一级项高亮：规则页下挂着 rules/new、rules/:id，精确匹配会让子页丢高亮；
 * 其余页面用路由名精确匹配（概览路径是 /，前缀匹配会误亮）。
 * @param name
 */
function isActive(name: string): boolean {
  if (name === "rules") return route.path.startsWith("/rules")
  return route.name === name
}

/**
 * 子项跳转目标：设置类子项带 ?section=，规则类子项直接按路由名跳
 * @param child
 */
function childLink(child: NavChild): RouteLocationRaw {
  return child.section !== undefined ? { name: child.name, query: { section: child.section } } : { name: child.name }
}

/**
 * 子项高亮：按路由名匹配；设置类子项在无 ?section 时回落到第一项
 * @param item
 * @param child
 */
function isChildActive(item: NavEntry, child: NavChild): boolean {
  if (route.name !== child.name) return false
  if (child.section === undefined) return true
  const current = route.query.section ?? item.children?.[0]?.section
  return current === child.section
}

/**
 * 展开态：手动开合优先；没动过就跟随当前路由是否命中该分组。
 * 路由变化命中分组时自动展开，离开不回滚（保留用户看到的层级）。
 */
const openState = reactive<Record<string, boolean>>({})
const isOpen = (item: NavEntry): boolean => openState[item.name] ?? isActive(item.name)
const setOpen = (item: NavEntry, value: boolean): void => {
  openState[item.name] = value
}

watch(
  () => route.fullPath,
  () => {
    for (const item of nav) {
      if (item.children?.length && isActive(item.name)) openState[item.name] = true
    }
  },
  { immediate: true }
)

/**
 * 分组标题点击：展开态切换子菜单；收起成图标栏时子菜单不可见，
 * 直接进第一个分区，否则「设置」图标会变成死按钮。
 * @param item
 */
function onGroupToggle(item: NavEntry): void {
  if (!item.children?.length) return
  if (state.value === "collapsed") {
    void router.push(childLink(item.children[0]))
    return
  }
  setOpen(item, !isOpen(item))
}
</script>

<template>
  <Sidebar collapsible="icon">
    <SidebarHeader class="p-2">
      <!-- 品牌块兼作窗口拖动区；收起时只留「锁」方块 -->
      <div
        data-tauri-drag-region
        class="flex h-9 items-center gap-2 rounded-md px-1 transition-[padding] duration-200 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
      >
        <div class="bg-primary text-primary-foreground grid size-7 shrink-0 place-items-center rounded-md">
          <span class="text-xs font-bold">锁</span>
        </div>
        <div class="min-w-0 leading-tight group-data-[collapsible=icon]:hidden">
          <div class="truncate text-sm font-semibold">莳间</div>
          <div class="text-muted-foreground truncate text-xs">florime</div>
        </div>
      </div>
    </SidebarHeader>

    <SidebarContent>
      <SidebarGroup>
        <SidebarGroupLabel>导航</SidebarGroupLabel>
        <SidebarMenu>
          <template v-for="item in nav" :key="item.name">
            <!-- 展开/收起自己接管：菜单按钮可能被 Tooltip 包一层，as-child 的触发器容易丢事件 -->
            <SidebarMenuItem v-if="item.children?.length">
              <Collapsible :open="isOpen(item)" class="group/collapsible" @update:open="(v: boolean) => setOpen(item, v)">
                <SidebarMenuButton :tooltip="item.label" :is-active="isActive(item.name)" @click="onGroupToggle(item)">
                  <component :is="item.icon" />
                  <span>{{ item.label }}</span>
                  <ChevronRight
                    class="ml-auto transition-transform duration-200 group-data-[collapsible=icon]:hidden group-data-[state=open]/collapsible:rotate-90"
                  />
                </SidebarMenuButton>
                <CollapsibleContent>
                  <SidebarMenuSub>
                    <SidebarMenuSubItem v-for="child in item.children" :key="child.name + (child.section ?? '')">
                      <SidebarMenuSubButton as-child :is-active="isChildActive(item, child)">
                        <RouterLink :to="childLink(child)">
                          <span>{{ child.title }}</span>
                        </RouterLink>
                      </SidebarMenuSubButton>
                    </SidebarMenuSubItem>
                  </SidebarMenuSub>
                </CollapsibleContent>
              </Collapsible>
            </SidebarMenuItem>

            <SidebarMenuItem v-else>
              <SidebarMenuButton as-child :tooltip="item.label" :is-active="isActive(item.name)">
                <RouterLink :to="{ name: item.name }">
                  <component :is="item.icon" />
                  <span>{{ item.label }}</span>
                </RouterLink>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </template>
        </SidebarMenu>
      </SidebarGroup>
    </SidebarContent>

    <SidebarFooter class="border-sidebar-border border-t">
      <!-- 折叠按钮：收起态只留图标居中 -->
      <Button
        variant="ghost"
        class="w-full justify-start gap-2 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
        @click="toggleSidebar"
      >
        <PanelLeft />
        <span class="group-data-[collapsible=icon]:hidden">收起侧栏</span>
      </Button>
    </SidebarFooter>

    <SidebarRail />
  </Sidebar>
</template>
