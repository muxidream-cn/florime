<script setup lang="ts">
import { Lock, ShieldAlert, TriangleAlert } from "@lucide/vue"
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatCountdown, formatDuration } from "@/utils/format"

import type { AlertPayload } from "@/types/models"

/**
 * 全屏形态的超限提醒。
 *
 * 窗口本身已经被后端切到全屏并置顶，这里只负责沉浸式呈现与三种操作出口。
 * 「不可关闭」时隐藏一切能收窗的按钮，只留倒计时与处置入口。
 */
const props = defineProps<{
  alert: AlertPayload
  /** 持续时间内的剩余秒数（仅不可关闭时有意义） */
  remaining: number
  closing: boolean
  closeFailed: boolean
  treeClosing: boolean
  treeFailed: boolean
}>()

const emit = defineEmits<{
  dismiss: []
  snooze: []
  closeNow: []
  forceCloseNow: []
}>()

const isCleanup = computed(() => props.alert.phase === "cleanup")
/** 不允许保留后台时，清理弹窗才提供进程树强制关闭入口 */
const canForceClose = computed(() => isCleanup.value && !props.alert.allow_background)
/** 只要按钮可能真正关闭程序，就必须醒目提醒先保存数据 */
const shouldWarnSave = computed(() => props.alert.action !== "notify_only" || canForceClose.value)

const saveHint = computed(() => {
  if (canForceClose.value) return "强制关闭进程树会立即结束未保存的进度。"
  return props.alert.action === "force_kill" ? "倒计时结束后将强制结束进程，未保存的进度会丢失。" : "倒计时结束后将关闭程序，未保存的进度可能丢失。"
})

const totalSecs = computed(() => Math.max(1, props.alert.auto_close_secs))

/** 可关闭时展示宽限期倒计时；不可关闭时展示持续时间倒计时 */
const now = ref(Date.now())
let tick: number | undefined
onMounted(() => {
  tick = window.setInterval(() => (now.value = Date.now()), 200)
})
onBeforeUnmount(() => {
  if (tick !== undefined) window.clearInterval(tick)
})

const graceRemaining = computed(() => Math.max(0, props.alert.grace_deadline_ts - now.value))
const shownSecs = computed(() => (props.alert.block_close ? props.remaining : Math.ceil(graceRemaining.value / 1000)))

const ratio = computed(() => {
  if (props.alert.block_close) {
    return Math.min(1, Math.max(0, props.remaining / totalSecs.value))
  }
  const total = Math.max(1, props.alert.grace_ms)
  return Math.min(1, Math.max(0, graceRemaining.value / total))
})

const radius = 66
const circumference = 2 * Math.PI * radius
const dash = computed(() => `${circumference * ratio.value} ${circumference}`)

const title = computed(() => {
  if (props.alert.message.trim()) return props.alert.message.trim()
  return isCleanup.value ? "后台还有进程在跑" : "今天的额度用完了"
})

const actionLabel = computed(() => {
  if (isCleanup.value) {
    const round = props.alert.round > 0 ? `第 ${props.alert.round} 轮复检：` : ""
    return canForceClose.value ? `${round}后台残留未清理；可强制关闭进程树` : `${round}允许保留后台，关闭后不再打扰`
  }
  switch (props.alert.action) {
    case "graceful_close":
      return "宽限期结束后自动关闭前台窗口"
    case "force_kill":
      return "宽限期结束后强制结束进程"
    default:
      return "不做处置，你也可以立即关闭它"
  }
})

const failHint = computed(() =>
  isCleanup.value ? "前台已关闭，后台仍有进程在跑，请自行关闭（提醒节奏不变）" : "没能关闭：进程可能受保护或已自行退出，可以再试一次"
)

const treeFailHint = computed(() => "强制关闭未成功：进程可能受保护或已自行退出，可以再试一次")

/** 持续时间小于宽限期：倒计时结束时弹窗会收起，但不代表宽限期结束，需要单独说明 */
const autoCloseLessThanGrace = computed(() => props.alert.block_close && props.alert.auto_close_secs * 1000 < props.alert.grace_ms)

/** 倒计时环下方的标签：不可关闭时展示持续时间，清理轮次与可关闭提醒展示阶段说明 */
const durationLabel = computed(() => {
  if (props.alert.block_close) return "持续时间"
  if (isCleanup.value) return "宽限期 · 后提醒"
  return "宽限期 · 后处置"
})
</script>

<template>
  <div
    class="animate-in fade-in relative flex h-full w-full flex-col items-center justify-center overflow-hidden bg-[#0b0709] text-zinc-100 duration-300 select-none"
  >
    <div
      class="pointer-events-none absolute inset-0"
      :class="
        isCleanup
          ? 'bg-[radial-gradient(circle_at_50%_35%,rgba(245,158,11,0.22),transparent_62%)]'
          : 'bg-[radial-gradient(circle_at_50%_35%,rgba(220,38,38,0.25),transparent_62%)]'
      "
    />
    <div
      class="pointer-events-none absolute -bottom-32 left-1/2 size-[36rem] -translate-x-1/2 rounded-full blur-3xl"
      :class="isCleanup ? 'bg-amber-500/10' : 'bg-destructive/10'"
    />

    <div class="relative flex flex-col items-center gap-7 px-10 text-center">
      <div class="flex items-center gap-2 text-xs tracking-[0.3em] uppercase" :class="isCleanup ? 'text-amber-300/70' : 'text-red-300/70'">
        <ShieldAlert class="size-3.5" />
        <span>{{ isCleanup ? "Waiting Cleanup" : "Limit Reached" }}</span>
      </div>

      <div class="flex flex-col items-center gap-3">
        <Badge v-if="isCleanup" variant="outline" class="border-amber-500/40 text-xs text-amber-300"> 等待后台清理 </Badge>
        <Badge v-else class="bg-destructive text-xs text-white">已超限</Badge>
        <h1 class="max-w-3xl text-4xl leading-tight font-semibold tracking-tight text-balance">
          {{ title }}
        </h1>
        <p class="text-sm text-zinc-400">
          {{ alert.display_name }}
          <span class="text-zinc-600">·</span>
          <span class="font-mono tabular-nums"> {{ formatDuration(alert.used_ms) }} / {{ formatDuration(alert.limit_ms) }} </span>
        </p>
      </div>

      <div class="relative grid place-items-center" style="width: 144px; height: 144px">
        <svg width="144" height="144" class="-rotate-90">
          <circle cx="72" cy="72" :r="radius" fill="none" stroke="currentColor" class="text-white/10" stroke-width="3" />
          <circle
            cx="72"
            cy="72"
            :r="radius"
            fill="none"
            stroke="currentColor"
            :class="['transition-[stroke-dasharray] duration-500 ease-linear', isCleanup ? 'text-amber-400' : 'text-red-400']"
            stroke-width="3"
            stroke-linecap="round"
            :stroke-dasharray="dash"
          />
        </svg>
        <div class="absolute inset-0 grid place-items-center">
          <div class="text-center">
            <div class="font-mono text-2xl font-semibold tabular-nums">
              {{ formatCountdown(shownSecs * 1000) }}
            </div>
            <div class="text-[0.6875rem] text-zinc-500">
              {{ durationLabel }}
            </div>
          </div>
        </div>
      </div>

      <div class="flex flex-col items-center gap-1.5">
        <div class="text-xs text-zinc-400">宽限期 {{ formatDuration(alert.grace_ms) }}</div>
        <p v-if="autoCloseLessThanGrace" class="max-w-sm text-[0.6875rem] leading-relaxed text-zinc-500">
          持续时间结束仅收起提醒，不代表宽限期结束；后台会在宽限期结束后开始处置。
        </p>
      </div>

      <div class="flex max-w-xl flex-col items-center gap-3">
        <!-- 会真正关闭程序时，提示必须比正文更抢眼：琥珀底 + 图标 + 加粗主句 -->
        <div
          v-if="shouldWarnSave"
          class="flex w-full items-start gap-2 rounded-lg border border-amber-400/40 bg-amber-400/10 px-3.5 py-2.5 text-left shadow-lg shadow-amber-500/10"
        >
          <TriangleAlert class="mt-0.5 size-4 shrink-0 text-amber-300" />
          <p class="text-xs leading-relaxed text-amber-100">
            <span class="font-semibold">请先保存数据。</span>
            <span class="text-amber-200/80">{{ saveHint }}</span>
          </p>
        </div>

        <p class="text-xs text-zinc-400">{{ actionLabel }}</p>
        <p v-if="isCleanup && alert.remaining > 0" class="text-xs text-amber-300">
          后台仍有 {{ alert.remaining }} 个进程，{{ canForceClose ? "可强制关闭进程树" : "允许保留则不再打扰" }}
        </p>
        <p v-if="closeFailed" class="text-xs text-red-300">{{ failHint }}</p>
        <p v-if="treeFailed" class="text-xs text-red-300">{{ treeFailHint }}</p>

        <!--
          「不可关闭」约束的是提醒弹窗本身：勾选后「我知道了 / 再给 5 分钟」全部隐藏，
          只能等倒计时。但「立即关闭」是处置动作（关进程而非关弹窗），
          无论是否不可关闭都保留手动关闭入口。
        -->
        <div class="flex flex-wrap items-center justify-center gap-2">
          <Button
            v-if="!alert.block_close"
            variant="ghost"
            size="sm"
            class="cursor-pointer text-zinc-300 hover:bg-white/10 hover:text-white"
            :disabled="closing || treeClosing"
            @click="emit('dismiss')"
          >
            我知道了
          </Button>
          <Button
            v-if="!isCleanup && !alert.block_close"
            size="sm"
            class="cursor-pointer bg-white/10 text-white hover:bg-white/20"
            :disabled="closing || treeClosing"
            @click="emit('snooze')"
          >
            再给 5 分钟
          </Button>
          <Button
            v-if="!canForceClose"
            size="sm"
            class="bg-destructive hover:bg-destructive/90 cursor-pointer text-white"
            :disabled="closing || treeClosing"
            @click="emit('closeNow')"
          >
            {{ closing ? "正在关闭…" : closeFailed ? "重试关闭" : "立即关闭" }}
          </Button>
          <Button
            v-if="canForceClose"
            size="sm"
            class="cursor-pointer border border-amber-300/50 bg-amber-400/20 font-semibold text-amber-100 hover:bg-amber-400/30"
            :disabled="closing || treeClosing"
            @click="emit('forceCloseNow')"
          >
            {{ treeClosing ? "正在强制关闭…" : treeFailed ? "重试强制关闭" : "强制关闭进程树" }}
          </Button>
        </div>
        <span v-if="alert.block_close" class="flex items-center gap-1.5 text-[0.6875rem] text-zinc-600">
          <Lock class="size-3" />
          {{
            alert.block_shortcuts ? "全屏提醒不可关闭，切出快捷键已暂时屏蔽" : "全屏提醒不可关闭，持续时间结束后收起；Ctrl+Alt+Del 等系统组合无法拦截"
          }}
        </span>
      </div>
    </div>
  </div>
</template>
