<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { cn } from "cn"

import { useSidebar } from "./utils"

/**
 * 侧栏容器（仅桌面形态）。
 *
 * 结构照搬参考项目：外层 `.group` 携带 data-collapsible，由 CSS 变体统一控制
 * 占位宽度与容器宽度；内层是 fixed 定位的实际侧栏。
 * 移动端 Sheet 分支与 floating / inset 变体在本项目用不到，已删。
 */
defineOptions({ inheritAttrs: false })

const props = withDefaults(
  defineProps<{
    side?: "left" | "right"
    collapsible?: "offcanvas" | "icon" | "none"
    class?: HTMLAttributes["class"]
  }>(),
  { side: "left", collapsible: "icon" },
)

const { state } = useSidebar()
</script>

<template>
  <div
    v-if="collapsible === 'none'"
    data-slot="sidebar"
    :class="
      cn(
        'flex w-(--sidebar-width) shrink-0 flex-col bg-sidebar text-sidebar-foreground',
        props.class,
      )
    "
    v-bind="$attrs"
  >
    <slot />
  </div>

  <div
    v-else
    class="group peer shrink-0 text-sidebar-foreground"
    data-slot="sidebar"
    :data-state="state"
    :data-collapsible="state === 'collapsed' ? collapsible : ''"
    data-variant="sidebar"
    :data-side="side"
  >
    <!-- 占位层：让主区随侧栏宽度平滑收缩 -->
    <div
      data-slot="sidebar-gap"
      :class="
        cn(
          'relative w-(--sidebar-width) bg-transparent transition-[width] duration-200 ease-linear',
          'group-data-[collapsible=icon]:w-(--sidebar-width-icon)',
          'group-data-[collapsible=offcanvas]:w-0',
        )
      "
    />

    <div
      data-slot="sidebar-container"
      :data-side="side"
      :class="
        cn(
          'fixed inset-y-0 z-10 flex w-(--sidebar-width) transition-[left,right,width] duration-200 ease-linear',
          side === 'left'
            ? 'left-0 group-data-[collapsible=offcanvas]:left-[calc(var(--sidebar-width)*-1)]'
            : 'right-0 group-data-[collapsible=offcanvas]:right-[calc(var(--sidebar-width)*-1)]',
          'group-data-[collapsible=icon]:w-(--sidebar-width-icon)',
          side === 'left' ? 'border-r border-sidebar-border' : 'border-l border-sidebar-border',
          props.class,
        )
      "
      v-bind="$attrs"
    >
      <div
        data-sidebar="sidebar"
        data-slot="sidebar-inner"
        class="flex size-full flex-col overflow-hidden bg-sidebar"
      >
        <slot />
      </div>
    </div>
  </div>
</template>
