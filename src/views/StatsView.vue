<script setup lang="ts">
import { Download, TrendingUp } from "@lucide/vue"
import { computed, onMounted, ref } from "vue"
import { toast } from "vue-sonner"

import Heatmap from "@/components/Heatmap.vue"
import TrendChart from "@/components/TrendChart.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { useStatsStore } from "@/stores/stats"
import { formatDuration, todayString } from "@/utils/format"

import type { DayStat } from "@/types/models"

const stats = useStatsStore()
const exporting = ref(false)

onMounted(async () => {
  await stats.loadRange(182)
  await stats.loadDetail(todayString())
})

/** 趋势图只画最近 30 天，且补全空缺日期，避免折线断档 */
const trend = computed<DayStat[]>(() => {
  const out: DayStat[] = []
  const map = stats.byDay
  const today = new Date()
  for (let i = 29; i >= 0; i--) {
    const d = new Date(today)
    d.setDate(d.getDate() - i)
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`
    out.push({ day: key, total_ms: map.get(key) ?? 0 })
  }
  return out
})

const topMax = computed(() => Math.max(1, ...stats.topExes.map((e) => e.total_ms)))

/** 历史三类互斥时长；颜色与首页环图保持一致 */
const categoryRows = computed(() => [
  {
    label: "专注",
    value: stats.categoryTotals.focusMs,
    dot: "bg-emerald-500",
    text: "text-emerald-600 dark:text-emerald-400",
  },
  {
    label: "摸鱼",
    value: stats.categoryTotals.slackMs,
    dot: "bg-amber-500",
    text: "text-amber-600 dark:text-amber-400",
  },
  {
    label: "专注摸鱼",
    value: stats.categoryTotals.focusSlackMs,
    dot: "bg-violet-500",
    text: "text-violet-600 dark:text-violet-400",
  },
])

/**
 *
 */
async function onExport(): Promise<void> {
  exporting.value = true
  try {
    // 后端直接写入系统下载文件夹，并在资源管理器中选中文件
    const path = await stats.exportCsv()
    toast.success("已导出到下载文件夹", { description: path })
  } catch (e) {
    toast.error("导出失败", { description: String(e) })
  } finally {
    exporting.value = false
  }
}

const detailMax = computed(() => Math.max(1, ...stats.detail.map((e) => e.total_ms)))
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold">统计</h1>
        <p class="text-muted-foreground text-xs">
          按时间轴去重统计规则命中时长：同一时刻只计一次，专注与摸鱼同时命中单独归类。单条规则的额度与提醒仍独立计算。数据只存在本机，不上传。
        </p>
      </div>
      <Button variant="outline" size="sm" :disabled="exporting" @click="onExport">
        <Download data-icon="inline-start" />
        导出 CSV
      </Button>
    </div>

    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <Card>
        <CardHeader class="pb-1">
          <CardDescription class="text-[0.8125rem]">累计去重时长</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="font-mono text-lg font-semibold tabular-nums">
            {{ formatDuration(stats.grandTotalMs) }}
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-1">
          <CardDescription class="text-[0.8125rem]">有效天数</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="font-mono text-lg font-semibold tabular-nums">{{ stats.activeDays }}</div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-1">
          <CardDescription class="text-[0.8125rem]">日均（有记录的天）</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="font-mono text-lg font-semibold tabular-nums">
            {{ formatDuration(stats.averageMs) }}
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-1">
          <CardDescription class="text-[0.8125rem]">单日最高</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="font-mono text-lg font-semibold tabular-nums">
            {{ formatDuration(stats.maxDayMs) }}
          </div>
        </CardContent>
      </Card>
    </div>

    <Card>
      <CardHeader class="pb-2">
        <CardTitle class="text-sm">分类汇总（近半年）</CardTitle>
        <CardDescription class="text-xs"> 三类互斥，专注 + 摸鱼 + 专注摸鱼 = 去重总时长 </CardDescription>
      </CardHeader>
      <CardContent>
        <div class="grid grid-cols-3 gap-3">
          <div v-for="item in categoryRows" :key="item.label" class="flex flex-col gap-1">
            <div class="text-muted-foreground flex items-center gap-1.5 text-xs">
              <span class="size-2 shrink-0 rounded-full" :class="item.dot" />
              {{ item.label }}
            </div>
            <div class="font-mono text-base font-semibold tabular-nums" :class="item.text">
              {{ formatDuration(item.value) }}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>

    <Card>
      <CardHeader>
        <CardTitle class="text-sm">使用日历</CardTitle>
        <CardDescription class="text-xs">点任意一天看当天明细</CardDescription>
      </CardHeader>
      <CardContent>
        <Heatmap :data="stats.byDay" :weeks="26" @select="stats.loadDetail($event)" />
      </CardContent>
    </Card>

    <div class="grid gap-4 lg:grid-cols-[1.6fr_1fr]">
      <Card>
        <CardHeader>
          <CardTitle class="flex items-center gap-2 text-sm">
            <TrendingUp class="size-4" />
            近 30 天趋势
          </CardTitle>
        </CardHeader>
        <CardContent>
          <TrendChart :points="trend" :height="200" />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle class="text-sm">Top 应用（近半年）</CardTitle>
        </CardHeader>
        <CardContent class="flex flex-col gap-2">
          <div v-if="stats.topExes.length === 0" class="text-muted-foreground text-xs">还没有数据</div>
          <div v-for="exe in stats.topExes" :key="exe.exe_key" class="flex flex-col gap-1">
            <div class="flex items-center justify-between text-[0.8125rem]">
              <span class="truncate">{{ exe.exe_name }}</span>
              <span class="text-muted-foreground ml-2 shrink-0 font-mono">
                {{ formatDuration(exe.total_ms, { compact: true }) }}
              </span>
            </div>
            <div class="bg-muted h-1.5 w-full overflow-hidden rounded-full">
              <div class="bg-primary/70 h-full rounded-full" :style="{ width: `${(exe.total_ms / topMax) * 100}%` }" />
            </div>
          </div>
        </CardContent>
      </Card>
    </div>

    <Card>
      <CardHeader>
        <CardTitle class="flex items-center gap-2 text-sm">
          {{ stats.selectedDay }} 明细
          <Badge variant="secondary" class="text-xs">按进程</Badge>
        </CardTitle>
      </CardHeader>
      <CardContent class="flex flex-col gap-1.5">
        <div v-if="stats.detail.length === 0" class="text-muted-foreground text-xs">这天没有记录</div>
        <div v-for="exe in stats.detail" :key="exe.exe_key" class="flex items-center gap-3">
          <span class="w-40 shrink-0 truncate text-[0.8125rem]">{{ exe.exe_name }}</span>
          <div class="bg-muted h-2 flex-1 overflow-hidden rounded-full">
            <div class="bg-primary h-full rounded-full" :style="{ width: `${(exe.total_ms / detailMax) * 100}%` }" />
          </div>
          <span class="w-20 shrink-0 text-right font-mono text-[0.8125rem] tabular-nums">
            {{ formatDuration(exe.total_ms, { compact: true }) }}
          </span>
        </div>
      </CardContent>
    </Card>
  </div>
</template>
