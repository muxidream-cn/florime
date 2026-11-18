<script setup lang="ts">
import { CircleAlert, Clock, Lock, TriangleAlert, X } from "@lucide/vue"
import { computed } from "vue"

import GraceCountdown from "@/components/GraceCountdown.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatDuration } from "@/utils/format"
import { usageTextClass, usageTone } from "@/utils/usage"

import type { AlertPayload } from "@/types/models"

/**
 * 小窗形态的超限提醒卡片。
 *
 * 全屏形态在 `AlertFullscreen.vue`，两者共用同一份载荷与操作语义，
 * 只是视觉与可用空间不同。
 */
const props = defineProps<{
  alert: AlertPayload
  /** 不可关闭时持续时间内的剩余秒数 */
  remaining: number
  closing: boolean
  closeFailed: boolean
  treeClosing: boolean
  treeFailed: boolean
  /** 进出场过渡开关（窗口是原生的，只能靠卡片自己淡入淡出） */
  visible: boolean
}>()

const emit = defineEmits<{
  dismiss: []
  snooze: []
  closeNow: []
  forceCloseNow: []
}>()

const blockClose = computed(() => props.alert.block_close)

/** 宽限期已过、仍在等后台清理：每轮复检都会重新弹出本窗 */
const isCleanup = computed(() => props.alert.phase === "cleanup")
/** 不允许保留后台时，清理弹窗才提供进程树强制关闭入口 */
const canForceClose = computed(() => isCleanup.value && !props.alert.allow_background)
/** 只要按钮可能真正关闭程序，就必须醒目提醒先保存数据 */
const shouldWarnSave = computed(() => props.alert.action !== "notify_only" || canForceClose.value)

/** 标题直接写「应用名超限提醒」，自定义文案仍在正文展示 */
const headerTitle = computed(() => `${props.alert.display_name}超限提醒`)

const countdownLabel = computed(() => (isCleanup.value ? "后提醒" : "后处置"))

/** 用量时间按百分比上色：提醒窗只在超限后出现，所以实际是红色，与概览页同一套阈值 */
const timeTone = computed(() => usageTone(props.alert.used_ms, props.alert.limit_ms))

const cleanupLine = computed(() =>
  isCleanup.value ? `后台仍有 ${props.alert.remaining} 个进程，${canForceClose.value ? "可强制关闭进程树" : "允许保留则不再打扰"}` : ""
)

const saveHint = computed(() => {
  if (canForceClose.value) return "强制关闭进程树会立即结束未保存的进度。"
  return props.alert.action === "force_kill" ? "倒计时结束后将强制结束进程，未保存的进度会丢失。" : "倒计时结束后将关闭程序，未保存的进度可能丢失。"
})

const actionLabel = computed(() => {
  if (isCleanup.value) {
    const round = props.alert.round > 0 ? `第 ${props.alert.round} 轮复检：` : ""
    return canForceClose.value ? `${round}后台残留未清理；可强制关闭进程树` : `${round}允许保留后台，关闭后不再打扰`
  }
  switch (props.alert.action) {
    case "graceful_close":
      return "宽限期结束后自动关闭"
    case "force_kill":
      return "宽限期结束后强制结束"
    default:
      return "不做处置，你也可以立即关闭它"
  }
})

/** 「立即关闭」没等到后端收窗时的原因提示：清理态是「前台关了、后台还在」 */
const failHint = computed(() =>
  isCleanup.value ? "前台已关闭，后台仍有进程在跑，请自行关闭（提醒节奏不变）" : "没能关闭：进程可能受保护或已自行退出，可以再试一次"
)

const treeFailHint = computed(() => "强制关闭未成功：进程可能受保护或已自行退出，可以再试一次")
</script>

<template>
  <div
    class="bg-card text-card-foreground ring-destructive/20 flex h-full flex-col overflow-hidden rounded-xl border shadow-2xl ring-1 transition-all duration-200 ease-out"
    :class="visible ? 'translate-y-0 opacity-100' : 'translate-y-2 opacity-0'"
  >
    <!-- 顶部进度线：提醒属于「阻断型」信息，用满宽色带强调 -->
    <div class="from-destructive via-destructive/70 h-0.5 w-full shrink-0 bg-gradient-to-r to-amber-500" />

    <div data-tauri-drag-region class="flex flex-col gap-1 px-3 pt-1 pb-2">
      <div class="flex items-center justify-between gap-2">
        <div data-tauri-drag-region class="flex min-w-0 items-center gap-1.5">
          <CircleAlert class="text-destructive size-3.5 shrink-0" />
          <span data-tauri-drag-region class="truncate text-xs font-medium">{{ headerTitle }}</span>
        </div>
        <button
          v-if="!blockClose"
          type="button"
          aria-label="关闭提醒"
          class="text-muted-foreground hover:bg-accent hover:text-foreground shrink-0 cursor-pointer rounded p-0.5 transition-colors duration-200"
          @click="emit('dismiss')"
        >
          <X class="size-3.5" />
        </button>
      </div>
      <!-- 拖拽柄 -->
      <div data-tauri-drag-region class="bg-border mx-auto h-0.5 w-8 rounded-full" />
    </div>

    <div class="flex flex-1 items-center gap-4 px-4 py-2">
      <GraceCountdown
        :deadline="alert.grace_deadline_ts"
        :total-ms="alert.grace_ms || 60_000"
        :label="countdownLabel"
        :tone="isCleanup ? 'warning' : 'danger'"
      />

      <div class="min-w-0 flex-1">
        <div class="truncate text-sm font-semibold">{{ alert.display_name }}</div>
        <div class="text-muted-foreground mt-0.5 truncate font-mono text-xs">
          {{ alert.exe_name }}
        </div>
        <div v-if="alert.message" class="text-foreground/80 mt-0.5 line-clamp-2 leading-snug">
          {{ alert.message }}
        </div>
        <div class="mt-1.5 flex items-center gap-2 text-[0.8125rem]">
          <Badge v-if="isCleanup" variant="outline" class="border-amber-500/40 text-xs text-amber-600 dark:text-amber-400"> 等待清理 </Badge>
          <Badge v-else variant="destructive" class="text-xs">已超限</Badge>
          <!-- 只有实际已用的时长染色；上限和分隔符保持中性 -->
          <span class="font-mono tabular-nums">
            <span :class="usageTextClass(timeTone)">{{ formatDuration(alert.used_ms) }}</span>
            <span class="text-muted-foreground"> / {{ formatDuration(alert.limit_ms) }} </span>
          </span>
        </div>
        <div v-if="cleanupLine" class="mt-1 text-[0.8125rem] font-medium text-amber-600 dark:text-amber-400">
          {{ cleanupLine }}
        </div>
        <!-- 会真正关闭程序时，琥珀底警示条比正文更抢眼：先保存再继续 -->
        <div v-if="shouldWarnSave" class="mt-1.5 flex items-start gap-1.5 rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-1.5">
          <TriangleAlert class="mt-0.5 size-3 shrink-0 text-amber-500" />
          <p class="text-[0.6875rem] leading-snug text-amber-700 dark:text-amber-300">
            <span class="font-semibold">请先保存数据。</span>{{ saveHint }}
          </p>
        </div>
        <div class="text-muted-foreground mt-1 flex flex-wrap items-center gap-1 text-xs">
          <Clock class="size-3 shrink-0" />
          {{ actionLabel }}
        </div>
        <div v-if="closeFailed" class="text-destructive mt-1 text-xs">
          {{ failHint }}
        </div>
        <div v-if="treeFailed" class="text-destructive mt-1 text-xs">
          {{ treeFailHint }}
        </div>
      </div>
    </div>

    <div class="flex flex-wrap gap-2 border-t px-3 py-2">
      <template v-if="!blockClose">
        <!-- 清理态用右上角 X 收窗即可，不重复放「先收窗」按钮 -->
        <Button
          v-if="!isCleanup"
          variant="ghost"
          size="sm"
          class="flex-1 cursor-pointer"
          :disabled="closing || treeClosing"
          title="收起提醒，宽限期照常继续"
          @click="emit('dismiss')"
        >
          我知道了
        </Button>
        <Button size="sm" class="flex-1 cursor-pointer" :disabled="closing || treeClosing" @click="emit('snooze')"> 再给 5 分钟 </Button>
        <Button
          v-if="!canForceClose"
          variant="outline"
          size="sm"
          class="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive flex-1 cursor-pointer"
          :disabled="closing || treeClosing"
          :title="isCleanup ? '只关闭当前目标，不重置提醒节奏' : '立即关闭该进程'"
          @click="emit('closeNow')"
        >
          {{ closing ? "正在关闭…" : closeFailed ? "重试关闭" : "立即关闭" }}
        </Button>
        <Button
          v-if="canForceClose"
          variant="destructive"
          size="sm"
          class="flex-1 cursor-pointer font-semibold"
          :disabled="closing || treeClosing"
          title="展开进程树并强制结束，未保存的数据将丢失"
          @click="emit('forceCloseNow')"
        >
          {{ treeClosing ? "正在强制关闭…" : treeFailed ? "重试强制关闭" : "强制关闭进程树" }}
        </Button>
      </template>
      <div v-else class="text-muted-foreground flex flex-1 items-center justify-center gap-1.5 py-1 text-xs">
        <Lock class="size-3 shrink-0" />
        持续时间剩余 {{ remaining }} 秒
      </div>
    </div>
  </div>
</template>
