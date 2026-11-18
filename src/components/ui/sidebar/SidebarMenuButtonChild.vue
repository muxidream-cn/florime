<script lang="ts">
import type { PrimitiveProps } from "reka-ui"
import type { HTMLAttributes } from "vue"
import type { SidebarMenuButtonVariants } from "."

/** 类型单独放在普通 script 里：`<script setup>` 不允许 ES 模块导出 */
export interface SidebarMenuButtonProps extends PrimitiveProps {
  variant?: SidebarMenuButtonVariants["variant"]
  size?: SidebarMenuButtonVariants["size"]
  isActive?: boolean
  class?: HTMLAttributes["class"]
}
</script>

<script setup lang="ts">
import { Primitive } from "reka-ui"
import { cn } from "cn"

import { sidebarMenuButtonVariants } from "."

const props = withDefaults(defineProps<SidebarMenuButtonProps>(), {
  as: "button",
  variant: "default",
  size: "default",
})
</script>

<template>
  <Primitive
    data-slot="sidebar-menu-button"
    data-sidebar="menu-button"
    :data-size="size"
    :data-active="isActive || undefined"
    :class="cn(sidebarMenuButtonVariants({ variant, size }), props.class)"
    :as="as"
    :as-child="asChild"
    v-bind="$attrs"
  >
    <slot />
  </Primitive>
</template>
