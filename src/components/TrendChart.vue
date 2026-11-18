<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { formatDuration } from "@/utils/format"

import type { DayStat } from "@/types/models"

const props = withDefaults(defineProps<{ points: DayStat[]; height?: number }>(), { height: 200 })

const host = ref<HTMLElement | null>(null)
const width = ref(620)
let observer: ResizeObserver | null = null

onMounted(() => {
  if (!host.value) return
  observer = new ResizeObserver((entries) => {
    const w = entries[0]?.contentRect.width ?? 0
    if (w > 0) width.value = Math.max(320, w)
  })
  observer.observe(host.value)
  width.value = Math.max(320, host.value.clientWidth)
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
})

const PAD_L = 42
const PAD_R = 10
const PAD_T = 12
const PAD_B = 22

const geometry = computed(() => {
  const pts = props.points
  const W = width.value
  const H = props.height
  const innerW = W - PAD_L - PAD_R
  const innerH = H - PAD_T - PAD_B

  const maxMs = Math.max(60_000, ...pts.map((p) => p.total_ms))
  // 取整到 30 分钟刻度，Y 轴数字才好看
  const stepMs = Math.max(30 * 60_000, Math.ceil(maxMs / 4 / (30 * 60_000)) * 30 * 60_000)

  const x = (i: number) => PAD_L + (pts.length <= 1 ? innerW / 2 : (i / (pts.length - 1)) * innerW)
  const y = (ms: number) => PAD_T + innerH - (ms / (stepMs * 4)) * innerH

  const line = pts.map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(p.total_ms).toFixed(1)}`).join(" ")
  const base = PAD_T + innerH
  const area = pts.length > 0 ? `${line} L${x(pts.length - 1).toFixed(1)},${base.toFixed(1)} L${x(0).toFixed(1)},${base.toFixed(1)} Z` : ""

  const yTicks = [0, 1, 2, 3, 4].map((k) => ({
    label: `${Math.round((stepMs * k) / 60_000)}m`,
    y: y(stepMs * k),
  }))

  const stride = Math.max(1, Math.ceil(pts.length / 6))
  const xTicks = pts
    .map((p, i) => ({ i, day: p.day }))
    .filter((p) => p.i % stride === 0 || p.i === pts.length - 1)
    .map((p) => ({ x: x(p.i), label: p.day.slice(5) }))

  const last = pts.length > 0 ? { x: x(pts.length - 1), y: y(pts[pts.length - 1].total_ms) } : null
  const xs = pts.map((_, i) => x(i))
  const ys = pts.map((p) => y(p.total_ms))

  return { W, H, PAD_L, line, area, yTicks, xTicks, last, xs, ys }
})

/** 悬浮索引：把鼠标的物理坐标换算成 viewBox 坐标后取最近点 */
const hoverIdx = ref<number | null>(null)

/**
 *
 * @param event
 */
function onMove(event: MouseEvent): void {
  const el = event.currentTarget as SVGSVGElement
  const rect = el.getBoundingClientRect()
  const g = geometry.value
  if (g.xs.length === 0 || rect.width <= 0) return
  const vx = ((event.clientX - rect.left) / rect.width) * g.W
  let best = 0
  let bestDist = Number.POSITIVE_INFINITY
  g.xs.forEach((px, i) => {
    const d = Math.abs(px - vx)
    if (d < bestDist) {
      bestDist = d
      best = i
    }
  })
  hoverIdx.value = best
}

const hoverPoint = computed(() => {
  const i = hoverIdx.value
  if (i === null) return null
  const point = props.points[i]
  const g = geometry.value
  if (!point || g.xs[i] === undefined) return null
  return { day: point.day, ms: point.total_ms, x: g.xs[i], y: g.ys[i] }
})
</script>

<template>
  <div ref="host" class="relative w-full">
    <svg
      v-if="points.length > 0"
      :viewBox="`0 0 ${geometry.W} ${geometry.H}`"
      :style="{ height: `${height}px`, width: '100%' }"
      @mousemove="onMove"
      @mouseleave="hoverIdx = null"
    >
      <defs>
        <linearGradient id="trend-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="currentColor" stop-opacity="0.3" />
          <stop offset="100%" stop-color="currentColor" stop-opacity="0.02" />
        </linearGradient>
      </defs>

      <g class="text-border">
        <line
          v-for="(t, i) in geometry.yTicks"
          :key="`g${i}`"
          :x1="geometry.PAD_L"
          :x2="geometry.W - 10"
          :y1="t.y"
          :y2="t.y"
          stroke="currentColor"
          stroke-width="1"
          stroke-dasharray="3 4"
        />
      </g>

      <g class="text-primary">
        <path :d="geometry.area" fill="url(#trend-fill)" />
        <path :d="geometry.line" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
        <circle v-if="geometry.last" :cx="geometry.last.x" :cy="geometry.last.y" r="3" fill="currentColor" />
      </g>

      <g v-if="hoverPoint">
        <line
          :x1="hoverPoint.x"
          :x2="hoverPoint.x"
          :y1="geometry.PAD_L - 8"
          :y2="geometry.H - 22"
          class="text-border"
          stroke="currentColor"
          stroke-width="1"
        />
        <circle
          :cx="hoverPoint.x"
          :cy="hoverPoint.y"
          r="3.5"
          class="text-primary"
          fill="currentColor"
          stroke="var(--background)"
          stroke-width="1.5"
        />
      </g>

      <g class="fill-muted-foreground" font-size="10">
        <text v-for="(t, i) in geometry.yTicks" :key="`y${i}`" x="6" :y="t.y + 3">
          {{ t.label }}
        </text>
        <text v-for="(t, i) in geometry.xTicks" :key="`x${i}`" :x="t.x" :y="geometry.H - 6" text-anchor="middle">
          {{ t.label }}
        </text>
      </g>
    </svg>

    <div
      v-if="hoverPoint"
      class="bg-popover pointer-events-none absolute z-10 min-w-16 -translate-x-1/2 rounded-md border px-2 py-1 text-xs shadow-md"
      :style="{
        left: `${(hoverPoint.x / geometry.W) * 100}%`,
        top: '4px',
      }"
    >
      <div class="text-muted-foreground">{{ hoverPoint.day }}</div>
      <div class="font-mono text-xs font-medium tabular-nums">
        {{ formatDuration(hoverPoint.ms, { compact: true }) }}
      </div>
    </div>

    <div v-else-if="points.length === 0" class="text-muted-foreground grid place-items-center text-xs" :style="{ height: `${height}px` }">
      还没有数据，先跑一天
    </div>
  </div>
</template>
