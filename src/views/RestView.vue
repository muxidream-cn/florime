<script setup lang="ts">
import { Coffee, Lock, X } from "@lucide/vue"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { getSettings, hideRest, restReady } from "@/api/commands"
import { EV, useTauriEvent } from "@/api/events"
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { applyTheme } from "@/lib/theme"
import { useRulesStore } from "@/stores/rules"
import { formatCountdown, formatDuration } from "@/utils/format"

import type { RestPayload } from "@/types/models"

/** 单条休息提醒：后端同一时刻只展示一条，后到覆盖先到 */
const payload = ref<RestPayload | null>(null)
/** 防止「倒计时到点」与「用户按钮」同时触发两次关闭处理 */
const ending = ref(false)
const rulesStore = useRulesStore()

/** 休息到点后的继续努力确认：未开启自动继续时才出现 */
const confirmOpen = ref(false)
const confirmRuleId = ref("")
const confirming = ref(false)

const remaining = ref(0)
let timer: number | undefined

const fullscreen = computed(() => payload.value?.fullscreen ?? false)
const totalSecs = computed(() => Math.max(1, payload.value?.auto_close_secs ?? 30))
const message = computed(() => payload.value?.message.trim() || "该休息一下了")
const usedText = computed(() => formatDuration(payload.value?.used_ms ?? 0))
const intervalMinutes = computed(() => Math.max(1, Math.round((payload.value?.interval_secs ?? 300) / 60)))
/** 环形进度：剩余比例 */
const ratio = computed(() => Math.min(1, Math.max(0, remaining.value / totalSecs.value)))
const ringRadius = computed(() => (fullscreen.value ? 132 : 76) / 2 - 6)
const ringCircumference = computed(() => 2 * Math.PI * ringRadius.value)
const ringDash = computed(() => `${ringCircumference.value * ratio.value} ${ringCircumference.value}`)

/**
 *
 */
function stopTimer(): void {
  if (timer !== undefined) window.clearInterval(timer)
  timer = undefined
}

/**
 *
 */
function startCountdown(): void {
  stopTimer()
  remaining.value = totalSecs.value
  timer = window.setInterval(() => {
    remaining.value -= 1
    if (remaining.value <= 0) {
      stopTimer()
      // 关闭自动继续时保持冻结，等后端 deadline 到点弹确认
      if (payload.value?.rest_auto_continue === false) return
      void finishRest()
    }
  }, 1000)
}

useTauriEvent<RestPayload>(EV.REST, (p) => {
  // 后端已经在 show 前推过一次，迟到/重复的同 id 事件不能再重置当前倒计时
  if (payload.value?.alert_id === p.alert_id) return
  payload.value = p
  ending.value = false
  startCountdown()
})

useTauriEvent<unknown>(EV.REST_CLOSED, () => {
  stopTimer()
  confirmOpen.value = false
  // 先隐藏再清内容，避免窗口还可见时露出空白 WebView
  void hideCurrentWindow().finally(() => {
    payload.value = null
    ending.value = false
  })
})

useTauriEvent<RestPayload>(EV.REST_CONFIRM, (p) => {
  stopTimer()
  if (!payload.value || payload.value.alert_id !== p.alert_id) payload.value = p
  remaining.value = 0
  confirmRuleId.value = p.rule_id
  ending.value = false
  confirmOpen.value = true
})

/**
 * Esc 是「允许关闭」时的快捷键出口；不可关闭时前端直接忽略
 * @param e
 */
function onKeydown(e: KeyboardEvent): void {
  if (e.key === "Escape" && !confirmOpen.value && payload.value && !payload.value.block_close) {
    e.preventDefault()
    void dismissRest()
  }
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown)
  try {
    const s = await getSettings()
    applyTheme(s.theme)
  } catch (e) {
    console.error("读取主题设置失败", e)
  }
  try {
    // 窗口首次创建时后端会挂起载荷，探针回来立即补弹
    await restReady()
  } catch (e) {
    console.error("休息提醒探针失败（capabilities 可能漏配 rest 窗）", e)
  }
})

onBeforeUnmount(() => {
  stopTimer()
  window.removeEventListener("keydown", onKeydown)
})

/**
 *
 */
async function hideCurrentWindow(): Promise<void> {
  try {
    await getCurrentWindow().hide()
  } catch {
    /* 窗口可能已销毁 */
  }
}

/** 普通关闭：只隐藏窗口，后端休息状态与倒计时保持到原持续时间结束 */
async function dismissRest(): Promise<void> {
  if (ending.value) return
  ending.value = true
  stopTimer()
  await hideCurrentWindow()
  payload.value = null
  ending.value = false
}

/** 自然结束：由后端统一收尾并恢复计时，避免前端先清内容导致空白窗 */
async function finishRest(): Promise<void> {
  if (ending.value) return
  ending.value = true
  stopTimer()
  try {
    await hideRest()
  } catch (e) {
    console.error("收起休息提醒失败", e)
    await hideCurrentWindow()
  }
  payload.value = null
  ending.value = false
}

/** 继续努力：取消后端休息状态并立即恢复计时 */
async function resumeWork(): Promise<void> {
  await finishRest()
}

/** 歇够了：直接收尾并恢复计时 */
async function confirmContinue(): Promise<void> {
  if (confirming.value) return
  confirming.value = true
  confirmOpen.value = false
  await finishRest()
  confirming.value = false
}

/** 取消：暂停触发休息的规则，然后收尾；Reload 也会幂等收起休息窗 */
async function confirmCancel(): Promise<void> {
  if (confirming.value) return
  confirming.value = true
  confirmOpen.value = false
  try {
    if (confirmRuleId.value) await rulesStore.setPaused(confirmRuleId.value, true)
  } catch (e) {
    console.error("暂停规则失败", e)
  }
  await finishRest()
  confirming.value = false
}
</script>

<template>
  <!-- 全屏：深色沉浸式，纯色背景 + 呼吸光晕 + 细环倒计时 -->
  <div
    v-if="payload && fullscreen"
    class="animate-in fade-in relative flex h-full w-full flex-col items-center justify-center overflow-hidden bg-[#070b0f] text-zinc-100 duration-300 select-none"
  >
    <div class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_38%,rgba(16,185,129,0.25),transparent_62%)]" />
    <div class="pointer-events-none absolute -bottom-32 left-1/2 size-[36rem] -translate-x-1/2 rounded-full bg-emerald-500/10 blur-3xl" />

    <div class="relative flex flex-col items-center gap-8 px-10 text-center">
      <div class="flex items-center gap-2 text-xs tracking-[0.3em] text-emerald-300/70 uppercase">
        <Coffee class="size-3.5" />
        <span>Rest Break</span>
      </div>

      <h1 class="max-w-3xl text-4xl leading-tight font-semibold tracking-tight text-balance">
        {{ message }}
      </h1>

      <p class="text-sm text-zinc-400">{{ payload.display_name }} 已专注 {{ usedText }} · 每 {{ intervalMinutes }} 分钟提醒一次</p>

      <div class="relative grid place-items-center" :style="{ width: '132px', height: '132px' }">
        <svg width="132" height="132" class="-rotate-90">
          <circle cx="66" cy="66" :r="ringRadius" fill="none" stroke="currentColor" class="text-white/10" stroke-width="3" />
          <circle
            cx="66"
            cy="66"
            :r="ringRadius"
            fill="none"
            stroke="currentColor"
            class="text-emerald-400 transition-[stroke-dasharray] duration-500 ease-linear"
            stroke-width="3"
            stroke-linecap="round"
            :stroke-dasharray="ringDash"
          />
        </svg>
        <div class="absolute inset-0 grid place-items-center">
          <div class="font-mono text-2xl font-semibold tabular-nums">
            {{ formatCountdown(remaining * 1000) }}
          </div>
        </div>
      </div>

      <div class="flex max-w-md flex-col items-center gap-3">
        <p class="text-xs text-zinc-500">
          {{
            payload.block_close
              ? "全屏提醒不可关闭，持续时间结束后收起；Ctrl+Alt+Del 等系统组合无法拦截"
              : "按 Esc 或点“我知道了”只关闭弹窗，休息状态持续到时间结束；点“继续努力”立即恢复计时"
          }}
        </p>
        <div v-if="!payload.block_close" class="flex flex-wrap items-center justify-center gap-2">
          <Button
            size="sm"
            variant="outline"
            class="cursor-pointer border-zinc-600 bg-transparent text-zinc-300 hover:bg-white/10 hover:text-white"
            @click="dismissRest"
          >
            我知道了
          </Button>
          <Button size="sm" class="cursor-pointer bg-emerald-500 text-emerald-950 hover:bg-emerald-400" @click="resumeWork"> 继续努力 </Button>
        </div>
        <span v-else class="flex items-center gap-1.5 text-[0.6875rem] text-zinc-600">
          <Lock class="size-3" />
          {{ payload.block_shortcuts ? "切出快捷键已暂时屏蔽" : "提醒期间建议离开屏幕活动一下" }}
        </span>
      </div>
    </div>
  </div>

  <!-- 小窗：与超限提醒一致的卡片形态 -->
  <div
    v-else-if="payload"
    class="bg-card text-card-foreground flex h-full flex-col overflow-hidden rounded-xl border shadow-2xl ring-1 ring-emerald-500/25"
  >
    <div class="h-0.5 w-full shrink-0 bg-gradient-to-r from-emerald-500 via-emerald-400 to-teal-300" />

    <div data-tauri-drag-region class="flex items-center justify-between gap-2 px-3 pt-2 pb-1">
      <div data-tauri-drag-region class="flex min-w-0 items-center gap-1.5">
        <Coffee class="size-3.5 shrink-0 text-emerald-600" />
        <span data-tauri-drag-region class="truncate text-xs font-medium">莳间 · 休息提醒</span>
      </div>
      <button
        v-if="!payload.block_close"
        type="button"
        aria-label="关闭提醒"
        class="text-muted-foreground hover:bg-accent hover:text-foreground shrink-0 cursor-pointer rounded p-0.5 transition-colors duration-200"
        @click="dismissRest"
      >
        <X class="size-3.5" />
      </button>
    </div>
    <div data-tauri-drag-region class="bg-border mx-auto h-0.5 w-8 rounded-full" />

    <div class="flex flex-1 items-center gap-4 px-4 py-2">
      <div class="relative grid shrink-0 place-items-center" :style="{ width: '76px', height: '76px' }">
        <svg width="76" height="76" class="-rotate-90">
          <circle cx="38" cy="38" :r="ringRadius" fill="none" stroke="currentColor" class="text-muted" stroke-width="5" />
          <circle
            cx="38"
            cy="38"
            :r="ringRadius"
            fill="none"
            stroke="currentColor"
            class="text-emerald-500 transition-[stroke-dasharray] duration-500 ease-linear"
            stroke-width="5"
            stroke-linecap="round"
            :stroke-dasharray="ringDash"
          />
        </svg>
        <div class="absolute inset-0 grid place-items-center">
          <div class="font-mono text-sm font-semibold tabular-nums">
            {{ formatCountdown(remaining * 1000) }}
          </div>
        </div>
      </div>

      <div class="min-w-0 flex-1">
        <div class="text-sm font-semibold break-words">{{ message }}</div>
        <div class="text-muted-foreground mt-1 truncate text-xs">{{ payload.display_name }} · 已专注 {{ usedText }}</div>
        <div class="text-muted-foreground mt-0.5 text-[0.6875rem]">每 {{ intervalMinutes }} 分钟提醒一次</div>
        <div v-if="!payload.block_close" class="text-muted-foreground mt-1 text-[0.625rem] leading-snug">
          普通关闭后休息状态持续到时间结束；点“继续努力”立即恢复计时
        </div>
      </div>
    </div>

    <div class="flex gap-2 border-t px-3 py-2">
      <template v-if="!payload.block_close">
        <Button size="sm" variant="outline" class="flex-1 cursor-pointer" @click="dismissRest"> 我知道了 </Button>
        <Button size="sm" class="flex-1 cursor-pointer bg-emerald-600 text-white hover:bg-emerald-600/90" @click="resumeWork"> 继续努力 </Button>
      </template>
      <div v-else class="text-muted-foreground flex flex-1 items-center justify-center gap-1.5 text-[0.6875rem]">
        <Lock class="size-3" />
        持续时间结束后收起
      </div>
    </div>
  </div>

  <AlertDialog :open="confirmOpen">
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>休息时间结束了</AlertDialogTitle>
        <AlertDialogDescription>是否继续努力？取消会暂停这条规则。</AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <Button size="sm" variant="outline" :disabled="confirming" @click="confirmCancel"> 取消并暂停规则 </Button>
        <Button size="sm" class="cursor-pointer bg-emerald-600 text-white hover:bg-emerald-600/90" :disabled="confirming" @click="confirmContinue">
          继续努力
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
