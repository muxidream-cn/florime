<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { formatCountdown } from "@/utils/format"

const props = withDefaults(
  defineProps<{
    /** 当前阶段结束的墙钟时间（epoch ms） */
    deadline: number
    totalMs?: number
    size?: number
    /** 倒计时说明文案（宽限期「后处置」，清理期「后提醒」） */
    label?: string
    /** 进度环色调：处置阶段用危险色，等待清理阶段用警示色 */
    tone?: "danger" | "warning"
  }>(),
  { totalMs: 60_000, size: 88, label: "后处置", tone: "danger" }
)

const now = ref(Date.now())
let timer: number | null = null

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now()
  }, 200)
})

onBeforeUnmount(() => {
  if (timer !== null) window.clearInterval(timer)
})

const remaining = computed(() => Math.max(0, props.deadline - now.value))
const ratio = computed(() => {
  if (props.totalMs <= 0) return 0
  return Math.min(1, Math.max(0, remaining.value / props.totalMs))
})

const radius = computed(() => (props.size - 8) / 2)
const circumference = computed(() => 2 * Math.PI * radius.value)
const dash = computed(() => `${circumference.value * ratio.value} ${circumference.value}`)
const ringClass = computed(() => (props.tone === "warning" ? "text-amber-500" : "text-destructive"))
</script>

<template>
  <div class="relative inline-grid place-items-center" :style="{ width: `${size}px`, height: `${size}px` }">
    <svg :width="size" :height="size" class="-rotate-90">
      <circle :cx="size / 2" :cy="size / 2" :r="radius" fill="none" stroke="currentColor" class="text-muted" stroke-width="7" />
      <circle
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        fill="none"
        stroke="currentColor"
        :class="['transition-[stroke-dasharray] duration-200', ringClass]"
        stroke-width="7"
        stroke-linecap="round"
        :stroke-dasharray="dash"
      />
    </svg>
    <div class="absolute inset-0 grid place-items-center">
      <div class="text-center">
        <div class="font-mono text-lg font-semibold tabular-nums">
          {{ formatCountdown(remaining) }}
        </div>
        <div class="text-muted-foreground text-[0.6875rem]">{{ label }}</div>
      </div>
    </div>
  </div>
</template>
