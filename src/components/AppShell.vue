<script setup lang="ts">
import { Pause, Play } from "@lucide/vue"
import { computed, onMounted } from "vue"
import { useRoute } from "vue-router"

import { getSettings } from "@/api/commands"
import { EV, useTauriEvent } from "@/api/events"
import AppSidebar from "@/components/AppSidebar.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"
import { applyTheme } from "@/lib/theme"
import { useMonitorStore } from "@/stores/monitor"
import { useSettingsStore } from "@/stores/settings"
import { formatDuration } from "@/utils/format"

import type { Snapshot } from "@/types/models"

const monitor = useMonitorStore()
const settingsStore = useSettingsStore()
const route = useRoute()

/** 编辑页要自己贴底的吸底栏，不能吃 main 的 20px 内边距 */
const fullBleed = computed(() => route.meta.fullBleed === true)

const statusText = computed(() => {
  if (monitor.resting) return "休息"
  if (monitor.paused) return "已暂停"
  if (monitor.stale) return "专注无响应"
  if (monitor.focusActive && monitor.slackActive) return "专注摸鱼中"
  if (monitor.focusActive) return "专注"
  if (monitor.slackActive) return "摸鱼"
  return "空闲"
})

const statusVariant = computed<"destructive" | "outline">(() => {
  if (monitor.resting || monitor.paused) return "outline"
  return monitor.stale ? "destructive" : "outline"
})

const statusClass = computed(() => {
  if (monitor.resting) {
    return "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
  }
  if (monitor.paused) {
    return "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400"
  }
  if (monitor.stale) return ""
  if (monitor.focusActive && monitor.slackActive) {
    return "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400"
  }
  if (monitor.focusActive) {
    return "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
  }
  if (monitor.slackActive) {
    return "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400"
  }
  return "border-muted-foreground/30 bg-muted/50 text-muted-foreground"
})

useTauriEvent<Snapshot>(EV.TICK, (payload) => monitor.setSnapshot(payload))
useTauriEvent<{ paused: boolean }>(EV.STATE, (payload) => monitor.setPaused(payload.paused))

onMounted(async () => {
  await monitor.checkCapabilities()
  await monitor.bootstrap()
  await settingsStore.load()
  try {
    const s = await getSettings()
    applyTheme(s.theme)
  } catch (e) {
    /* 首帧主题已由 main.ts 兜底，这里只记录原因 */
    console.error("读取主题失败", e)
  }
})
</script>

<template>
  <SidebarProvider>
    <AppSidebar />

    <SidebarInset>
      <header data-tauri-drag-region class="flex h-14 shrink-0 items-center justify-between gap-3 border-b px-4">
        <div class="flex items-center gap-2">
          <Badge :variant="statusVariant" :class="statusClass">
            {{ statusText }}
          </Badge>
          <span class="text-muted-foreground text-xs">
            {{ monitor.snapshot.trigger_mode === "event" ? "事件触发" : "周期轮询" }}
          </span>
          <Badge
            v-if="!monitor.elevated"
            variant="outline"
            class="border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400"
            title="当前非管理员：无权处置以管理员身份运行的进程"
          >
            非管理员
          </Badge>
          <Badge v-if="monitor.health === 'error'" variant="destructive" title="窗口 IPC 权限未生效，请检查 capabilities 配置"> 窗口权限异常 </Badge>
        </div>

        <div class="flex items-center gap-3">
          <div class="text-muted-foreground text-xs">
            今日累计
            <span class="text-foreground ml-1 font-mono text-sm font-medium">
              {{ formatDuration(monitor.todayTotalMs) }}
            </span>
          </div>
          <Button size="sm" variant="outline" @click="monitor.togglePause()">
            <component :is="monitor.paused ? Play : Pause" data-icon="inline-start" />
            {{ monitor.paused ? "继续" : "暂停" }}
          </Button>
        </div>
      </header>

      <main :class="fullBleed ? 'min-h-0 flex-1 overflow-y-auto' : 'min-h-0 flex-1 overflow-y-auto p-5'">
        <RouterView />
      </main>
    </SidebarInset>
  </SidebarProvider>
</template>
