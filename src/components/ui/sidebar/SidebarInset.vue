<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { cn } from "cn"

const props = defineProps<{
  class?: HTMLAttributes["class"]
}>()
</script>

<template>
  <!-- 根元素用 div：真正的滚动容器仍是 AppShell 里的 main -->
  <div
    data-slot="sidebar-inset"
    :class="cn('relative flex min-w-0 flex-1 flex-col overflow-hidden', props.class)"
  >
    <slot />
  </div>
</template>
