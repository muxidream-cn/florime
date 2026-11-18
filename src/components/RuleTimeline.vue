<script setup lang="ts">
import { computed } from "vue"

import { formatCountdown } from "@/utils/format"
import { usageBarClass, usageTextClass, type RuleTimelineItem } from "@/utils/usage"

/**
 * 概览卡片上的一条倒计时时间线：标签 + 等宽倒计时 + 细进度条。
 *
 * 纯展示组件：剩余量、总量与百分比都由父层按快照算好传进来，
 * 这样「暂停 / 休息冻结」时倒计时会自然停住，不需要额外的定时器。
 */
const props = defineProps<RuleTimelineItem>()

const textClass = computed(() => (props.tone === "focus" ? "text-emerald-600 dark:text-emerald-400" : usageTextClass(props.tone)))

const barClass = computed(() => (props.tone === "focus" ? "bg-gradient-to-r from-emerald-500 to-teal-400" : usageBarClass(props.tone)))

const dotClass = computed(() => (props.tone === "focus" ? "bg-emerald-500" : usageBarClass(props.tone)))

const width = computed(() => `${Math.min(100, Math.max(0, props.percent))}%`)

/** 额度用尽后不再倒数，直接说明状态，避免一直停在 00:00 */
const valueText = computed(() => {
  if (props.id === "limit" && props.remainingMs <= 0) return "已超额"
  if (props.id === "over") return `已超 ${formatCountdown(props.remainingMs)}`
  return formatCountdown(props.remainingMs)
})
</script>

<template>
  <div class="flex flex-col gap-1">
    <div class="flex items-baseline justify-between gap-2">
      <span class="text-muted-foreground flex min-w-0 items-center gap-1.5 text-[0.6875rem]">
        <span class="size-1.5 shrink-0 rounded-full" :class="dotClass" />
        <span class="truncate">{{ label }}</span>
      </span>
      <span class="shrink-0 font-mono text-xs font-medium tabular-nums" :class="textClass" :title="`共 ${Math.round(totalMs / 60000)} 分钟`">
        {{ valueText }}
      </span>
    </div>
    <div class="bg-muted h-0.5 w-full overflow-hidden rounded-full">
      <div class="h-full rounded-full transition-[width] duration-500" :class="barClass" :style="{ width }" />
    </div>
  </div>
</template>
