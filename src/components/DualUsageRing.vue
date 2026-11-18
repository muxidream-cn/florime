<script setup lang="ts">
import { computed } from "vue"

/**
 * 今日累计环：外圈只有一条，按时间轴去重后的三类互斥占比切分。
 *
 * - 仅专注（绿）
 * - 仅摸鱼（琥珀）
 * - 专注摸鱼同时命中（紫）
 *
 * 三段相加正好等于总时长；规则各自的额度仍只在规则用量卡片里展示。
 */
const props = withDefaults(
  defineProps<{
    focusMs: number
    slackMs: number
    focusSlackMs: number
    size?: number
    /** 环心主数字；不传则显示合计时长由父组件决定 */
    value?: string
    caption?: string
  }>(),
  { size: 152, value: "", caption: "" }
)

const STROKE = 12

const center = computed(() => props.size / 2)
const radius = computed(() => props.size / 2 - STROKE / 2 - 1)
const circumferencePx = computed(() => 2 * Math.PI * radius.value)

const segments = computed(() => {
  const values = [props.focusMs, props.slackMs, props.focusSlackMs].map((v) => Math.max(0, v || 0))
  const total = values.reduce((a, b) => a + b, 0)
  if (total <= 0) return []

  const c = circumferencePx.value
  const colors = ["text-emerald-500", "text-amber-500", "text-violet-500"]
  let offset = 0
  const out: { key: string; dash: string; offset: number; color: string }[] = []
  values.forEach((value, i) => {
    if (value <= 0) return
    const length = (value / total) * c
    out.push({
      key: ["focus", "slack", "focus_slack"][i],
      dash: `${length} ${c - length}`,
      offset: -offset,
      color: colors[i],
    })
    offset += length
  })
  return out
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
      <circle :cx="center" :cy="center" :r="radius" fill="none" stroke="currentColor" class="text-muted" :stroke-width="STROKE" />
      <circle
        v-for="segment in segments"
        :key="segment.key"
        :cx="center"
        :cy="center"
        :r="radius"
        fill="none"
        stroke="currentColor"
        :class="`${segment.color} transition-[stroke-dasharray,stroke-dashoffset] duration-700 ease-out`"
        :stroke-width="STROKE"
        :stroke-dasharray="segment.dash"
        :stroke-dashoffset="segment.offset"
      />
    </svg>

    <div class="absolute inset-0 grid place-items-center text-center">
      <div class="px-1">
        <div class="font-mono font-semibold whitespace-nowrap tabular-nums" :class="valueClass">
          {{ value }}
        </div>
        <div v-if="caption" class="text-muted-foreground mt-0.5 text-xs leading-tight whitespace-nowrap">
          {{ caption }}
        </div>
      </div>
    </div>
  </div>
</template>
