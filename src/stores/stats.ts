import { defineStore } from "pinia"
import { computed, ref } from "vue"

import * as api from "@/api/commands"
import { daysAgo, todayString } from "@/utils/format"

import type { DayCategoryStat, DayStat, ExeStat } from "@/types/models"

export const useStatsStore = defineStore("stats", () => {
  const days = ref<DayStat[]>([])
  const categoryStats = ref<DayCategoryStat[]>([])
  const topExes = ref<ExeStat[]>([])
  const detail = ref<ExeStat[]>([])
  const grandTotalMs = ref(0)
  const selectedDay = ref(todayString())
  const loading = ref(false)
  const error = ref("")

  const byDay = computed(() => {
    const map = new Map<string, number>()
    for (const d of days.value) map.set(d.day, d.total_ms)
    return map
  })

  const maxDayMs = computed(() => days.value.reduce((acc, d) => Math.max(acc, d.total_ms), 0))

  const activeDays = computed(() => days.value.filter((d) => d.total_ms > 0).length)

  const averageMs = computed(() => (activeDays.value === 0 ? 0 : Math.round(days.value.reduce((a, d) => a + d.total_ms, 0) / activeDays.value)))

  /** 近半年三类互斥时长的合计，与累计时长同一去重口径 */
  const categoryTotals = computed(() => ({
    focusMs: categoryStats.value.reduce((a, d) => a + d.focus_ms, 0),
    slackMs: categoryStats.value.reduce((a, d) => a + d.slack_ms, 0),
    focusSlackMs: categoryStats.value.reduce((a, d) => a + d.focus_slack_ms, 0),
    totalMs: categoryStats.value.reduce((a, d) => a + d.total_ms, 0),
  }))

  /**
   * 拉取近 N 天数据（热力图用）
   * @param daysBack
   */
  async function loadRange(daysBack = 182): Promise<void> {
    loading.value = true
    error.value = ""
    try {
      const from = daysAgo(daysBack)
      const to = todayString()
      const [stats, categories, top, total] = await Promise.all([
        api.queryDayStats(from, to),
        api.queryDayCategoryStats(from, to),
        api.queryTopExes(from, to, 8),
        api.queryGrandTotal(),
      ])
      days.value = stats
      categoryStats.value = categories
      topExes.value = top
      grandTotalMs.value = total
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  /**
   *
   * @param day
   */
  async function loadDetail(day: string): Promise<void> {
    selectedDay.value = day
    try {
      detail.value = await api.queryDayDetail(day)
    } catch (e) {
      error.value = String(e)
    }
  }

  /**
   *
   */
  async function exportCsv(): Promise<string> {
    return api.exportCsv(daysAgo(30), todayString())
  }

  return {
    days,
    categoryStats,
    topExes,
    detail,
    grandTotalMs,
    selectedDay,
    loading,
    error,
    byDay,
    maxDayMs,
    activeDays,
    averageMs,
    categoryTotals,
    loadRange,
    loadDetail,
    exportCsv,
  }
})
