<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { PanelLeft } from "@lucide/vue"
import { cn } from "cn"

import { Button } from "@/components/ui/button"

import { useSidebar } from "./utils"

const props = defineProps<{
  class?: HTMLAttributes["class"]
}>()

const { toggleSidebar } = useSidebar()
</script>

<template>
  <Button
    data-sidebar="trigger"
    data-slot="sidebar-trigger"
    variant="ghost"
    size="icon-sm"
    class="cursor-pointer"
    :class="cn('', props.class)"
    aria-label="收起或展开侧栏"
    @click="toggleSidebar"
  >
    <PanelLeft />
  </Button>
</template>
