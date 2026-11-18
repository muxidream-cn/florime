<script setup lang="ts">
import { computed } from "vue"

const props = withDefaults(
  defineProps<{
    used: number
    limit: number
    size?: number
    stroke?: number
    tone?: "normal" | "warning" | "danger"
    caption?: string
    /** 环心主数字；不传则显示百分比 */
    value?: string
  }>(),
  { size: 104, stroke: 8, tone: "normal", caption: "", value: "" }
)

const radius = computed(() => (props.size - props.stroke) / 2)
const circumference = computed(() => 2 * Math.PI * radius.value)
const ratio = computed(() => {
  if (props.limit <= 0) return 0
  return Math.min(1, Math.max(0, props.used / props.limit))
})
const dash = computed(() => `${circumference.value * ratio.value} ${circumference.value}`)

const toneClass = computed(() => {
  if (props.tone === "danger") return "text-destructive"
  if (props.tone === "warning") return "text-amber-500"
  // 正常态与规则卡进度条同色：用量还没到 80% 就是绿色
  return "text-emerald-500"
})

/** 环心主文案：内容越长字号越小，保证「12 小时 34 分」这类长串不出圈 */
const valueClass = computed(() => {
  const text = props.value
  if (!text) return "text-sm leading-tight"
  if (text.length >= 9) return "text-sm leading-tight"
  if (text.length >= 7) return "text-base leading-tight"
  return "text-lg leading-tight"
})
</script>

<template>
  <div class="relative inline-grid place-items-center" :style="{ width: `${size}px`, height: `${size}px` }">
    <svg :width="size" :height="size" class="-rotate-90">
      <circle :cx="size / 2" :cy="size / 2" :r="radius" fill="none" stroke="currentColor" class="text-muted" :stroke-width="stroke" />
      <circle
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        fill="none"
        stroke="currentColor"
        :class="[toneClass, 'transition-[stroke-dasharray] duration-500']"
        :stroke-width="stroke"
        stroke-linecap="round"
        :stroke-dasharray="dash"
      />
    </svg>
    <div class="absolute inset-0 grid place-items-center text-center">
      <div class="px-1">
        <div class="font-mono font-semibold whitespace-nowrap tabular-nums" :class="valueClass">
          {{ value || `${Math.round(ratio * 100)}%` }}
        </div>
        <div v-if="caption" class="text-muted-foreground mt-0.5 text-xs leading-tight whitespace-nowrap">
          {{ caption }}
        </div>
      </div>
    </div>
  </div>
</template>
