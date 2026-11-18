<script setup lang="ts">
import { cn } from "cn"
import type { HTMLAttributes } from "vue"

const props = defineProps<{ class?: HTMLAttributes["class"] }>()
</script>

<template>
  <div data-slot="skeleton" :class="cn('bg-muted animate-pulse rounded-md', props.class)" />
</template>
