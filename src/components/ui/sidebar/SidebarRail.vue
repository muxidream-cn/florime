<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { cn } from "cn"

import { useSidebar } from "./utils"

const props = defineProps<{
  class?: HTMLAttributes["class"]
}>()

const { toggleSidebar, state } = useSidebar()
</script>

<template>
  <button
    data-sidebar="rail"
    data-slot="sidebar-rail"
    aria-label="收起或展开侧栏"
    :tabindex="-1"
    :title="state === 'expanded' ? '收起侧栏' : '展开侧栏'"
    :class="
      cn(
        'absolute inset-y-0 -right-4 z-20 hidden w-4 transition-all ease-linear sm:flex',
        'after:absolute after:inset-y-0 after:left-1/2 after:w-0.5 hover:after:bg-sidebar-border',
        'cursor-w-resize [[data-state=collapsed]_&]:cursor-e-resize',
        props.class,
      )
    "
    @click="toggleSidebar"
  >
    <slot />
  </button>
</template>
