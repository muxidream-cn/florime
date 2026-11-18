<script setup lang="ts">
import { useEventListener } from "@vueuse/core"
import { computed, ref, type HTMLAttributes } from "vue"
import { TooltipProvider } from "reka-ui"
import { cn } from "cn"

import {
  provideSidebarContext,
  SIDEBAR_KEYBOARD_SHORTCUT,
  SIDEBAR_STORAGE_KEY,
  SIDEBAR_WIDTH,
  SIDEBAR_WIDTH_ICON,
} from "./utils"

/**
 * 侧栏状态容器：注入宽度变量、提供上下文、接管 Ctrl/Cmd+B。
 *
 * 与参考项目的差异：状态持久化用 localStorage 而不是 cookie；
 * 不做移动端分支；外层高度用 h-full 跟随窗口，避免 min-h-svh 撑出第二层滚动。
 */
const props = defineProps<{
  class?: HTMLAttributes["class"]
}>()

/** 存过 false 才算收起；读不到（首次启动 / 存储被禁）一律回落为展开 */
function readStoredOpen(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_STORAGE_KEY) !== "false"
  } catch (e) {
    console.error("读取侧栏展开状态失败", e)
    return true
  }
}

/**
 * 展开态由 Provider 独占，不做外部受控。
 *
 * 注意：不要在这里声明 `open?: boolean` 再判断 `props.open !== undefined`——
 * Vue 会把缺失的 Boolean prop 转成 false（不是 undefined），
 * 于是组件会被误判为「受控且恒为 false」，点击切换永远改不动宽度。
 */
const open = ref(readStoredOpen())

function setOpen(value: boolean): void {
  open.value = value
  try {
    window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(value))
  } catch (e) {
    console.error("保存侧栏展开状态失败", e)
  }
}

function toggleSidebar(): void {
  setOpen(!open.value)
}

const state = computed<"expanded" | "collapsed">(() => (open.value ? "expanded" : "collapsed"))

useEventListener("keydown", (event: KeyboardEvent) => {
  // 只认单纯的 Ctrl/Cmd+B：带 Shift/Alt 的组合留给系统或输入法
  if (event.key.toLowerCase() !== SIDEBAR_KEYBOARD_SHORTCUT) return
  if (!(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) return
  event.preventDefault()
  toggleSidebar()
})

provideSidebarContext({ state, open, setOpen, toggleSidebar })
</script>

<template>
  <TooltipProvider :delay-duration="0">
    <div
      data-slot="sidebar-wrapper"
      :style="{
        '--sidebar-width': SIDEBAR_WIDTH,
        '--sidebar-width-icon': SIDEBAR_WIDTH_ICON,
      }"
      :class="cn('group/sidebar-wrapper flex h-full w-full', props.class)"
    >
      <slot />
    </div>
  </TooltipProvider>
</template>
