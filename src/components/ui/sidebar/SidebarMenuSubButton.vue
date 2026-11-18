<script setup lang="ts">
import type { PrimitiveProps } from "reka-ui"
import type { HTMLAttributes } from "vue"
import { Primitive } from "reka-ui"
import { cn } from "cn"

const props = withDefaults(
  defineProps<
    PrimitiveProps & {
      size?: "sm" | "md"
      isActive?: boolean
      class?: HTMLAttributes["class"]
    }
  >(),
  { as: "a", size: "md" },
)
</script>

<template>
  <Primitive
    data-slot="sidebar-menu-sub-button"
    data-sidebar="menu-sub-button"
    :as="as"
    :as-child="asChild"
    :data-size="size"
    :data-active="isActive || undefined"
    :class="
      cn(
        'flex h-7 min-w-0 -translate-x-px items-center gap-2 overflow-hidden rounded-md px-2 text-xs text-sidebar-foreground outline-hidden',
        'hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
        'data-active:bg-sidebar-accent data-active:font-medium data-active:text-sidebar-accent-foreground',
        'focus-visible:ring-2 focus-visible:ring-sidebar-ring',
        'group-data-[collapsible=icon]:hidden',
        'disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50',
        '[&>span:last-child]:truncate [&>svg]:size-4 [&>svg]:shrink-0',
        props.class,
      )
    "
  >
    <slot />
  </Primitive>
</template>
