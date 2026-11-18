<script setup lang="ts">
import { computed, ref } from "vue"

import { formatDuration, toDayString } from "@/utils/format"

const props = withDefaults(
  defineProps<{
    /** day(YYYY-MM-DD) -> 毫秒 */
    data: Map<string, number>
    weeks?: number
  }>(),
  { weeks: 26 }
)

const emit = defineEmits<{ select: [day: string] }>()

const hover = ref<{ day: string; ms: number } | null>(null)

/** 色阶：绝对阈值比相对阈值更好读（不随最高值漂移） */
const LEVELS = [0, 15 * 60_000, 45 * 60_000, 90 * 60_000, 180 * 60_000, 300 * 60_000]

/**
 *
 * @param ms
 */
function levelOf(ms: number): number {
  if (ms <= 0) return 0
  if (ms < LEVELS[1]) return 1
  if (ms < LEVELS[2]) return 2
  if (ms < LEVELS[3]) return 3
  if (ms < LEVELS[4]) return 4
  return 5
}

const LEVEL_CLASS = ["bg-muted", "bg-primary/20", "bg-primary/40", "bg-primary/60", "bg-primary/80", "bg-primary"]

interface Cell {
  day: string
  ms: number
  level: number
  future: boolean
  today: boolean
}

const todayStr = toDayString(new Date())

/** 按列（周）× 行（周一..周日）铺满 */
const columns = computed<Cell[][]>(() => {
  const end = new Date()
  const total = props.weeks * 7
  const start = new Date(end)
  start.setDate(start.getDate() - (total - 1))

  // 对齐到周一
  const offset = (start.getDay() + 6) % 7
  start.setDate(start.getDate() - offset)

  const cols: Cell[][] = []
  const cursor = new Date(start)
  for (let w = 0; w < props.weeks; w++) {
    const col: Cell[] = []
    for (let d = 0; d < 7; d++) {
      const day = toDayString(cursor)
      const ms = props.data.get(day) ?? 0
      col.push({
        day,
        ms,
        level: levelOf(ms),
        future: cursor.getTime() > end.getTime(),
        today: day === todayStr,
      })
      cursor.setDate(cursor.getDate() + 1)
    }
    cols.push(col)
  }
  return cols
})

const monthLabels = computed(() => {
  const labels: { index: number; text: string; tail: boolean }[] = []
  let lastMonth = -1
  columns.value.forEach((col, i) => {
    const first = col.find((c) => !c.future) ?? col[0]
    const d = new Date(`${first.day}T00:00:00`)
    const m = d.getMonth()
    if (m !== lastMonth) {
      lastMonth = m
      // 末尾两列的标签向左展开，避免冲出容器
      labels.push({ index: i, text: `${m + 1} 月`, tail: i >= props.weeks - 2 })
    }
  })
  return labels
})

const WEEKDAYS = ["一", "", "三", "", "五", "", "日"]
</script>

<template>
  <div class="w-full">
    <div class="text-muted-foreground mb-1 flex gap-1 text-xs">
      <!-- 与星期轴同宽的占位，保证月份和方格左对齐 -->
      <div class="w-3 shrink-0" />
      <!-- 月份轴：独立行高，避免被下方方格压住 -->
      <div class="relative h-4 flex-1">
        <span
          v-for="label in monthLabels"
          :key="`${label.index}-${label.text}`"
          class="absolute top-0 leading-4 whitespace-nowrap"
          :style="{
            left: `${(label.index / weeks) * 100}%`,
            transform: label.tail ? 'translateX(-100%)' : undefined,
          }"
        >
          {{ label.text }}
        </span>
      </div>
    </div>

    <div class="flex gap-1">
      <!-- 星期轴 -->
      <div class="text-muted-foreground flex w-3 shrink-0 flex-col justify-between pt-0.5 text-[0.6875rem] leading-[0.75rem]">
        <span v-for="(w, i) in WEEKDAYS" :key="i" class="h-[0.75rem]">{{ w }}</span>
      </div>

      <div class="flex min-w-0 flex-1 gap-[3px]">
        <div v-for="(col, ci) in columns" :key="ci" class="flex flex-1 flex-col gap-[3px]">
          <div
            v-for="cell in col"
            :key="cell.day"
            class="aspect-square w-full rounded-[3px] transition-transform hover:scale-110"
            :class="[
              cell.future ? 'bg-muted/40' : LEVEL_CLASS[cell.level],
              cell.today ? 'ring-foreground/50 ring-1' : '',
              cell.future ? 'cursor-default' : 'cursor-pointer',
            ]"
            @mouseenter="hover = { day: cell.day, ms: cell.ms }"
            @mouseleave="hover = null"
            @click="!cell.future && emit('select', cell.day)"
          />
        </div>
      </div>
    </div>

    <div class="text-muted-foreground mt-2 flex h-4 items-center justify-between text-xs">
      <span v-if="hover">{{ hover.day }} · {{ formatDuration(hover.ms, { compact: true }) }}</span>
      <span v-else>近 {{ weeks }} 周</span>
      <span class="flex items-center gap-2">
        <span class="flex items-center gap-1">
          少
          <span v-for="(cls, i) in LEVEL_CLASS" :key="i" class="size-2.5 rounded-[2px]" :class="cls" />
          多
        </span>
        <span class="flex items-center gap-1">
          <span class="bg-muted/40 size-2.5 rounded-[2px]" />
          未来
        </span>
      </span>
    </div>
  </div>
</template>
