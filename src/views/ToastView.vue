<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window"
import { onBeforeUnmount, onMounted, ref } from "vue"

import { dismissAlert, enforceNow, forceKillTree, getActiveAlert, getSettings, ping, snoozeRule } from "@/api/commands"
import { EV, useTauriEvent } from "@/api/events"
import AlertFullscreen from "@/components/AlertFullscreen.vue"
import ToastCard from "@/components/ToastCard.vue"
import { applyTheme } from "@/lib/theme"

import type { AlertPayload, Snapshot } from "@/types/models"

/**
 * 超限提醒窗的容器：只负责事件、倒计时与操作指令。
 *
 * 视觉按规则拆成两套：小窗卡片（ToastCard）与全屏沉浸（AlertFullscreen），
 * 后端已经把窗口切成对应形态，这里只决定渲染谁。
 */
const alert = ref<AlertPayload | null>(null)
/** 进出场过渡：窗口是原生的，靠卡片自己淡入淡出掩盖瞬间显示/隐藏 */
const visible = ref(false)
/** 「立即关闭」等待后端回执中 */
const closing = ref(false)
/** 后端没有在超时内确认收窗（受保护 / 无权限 / 已自行退出） */
const closeFailed = ref(false)
/** 「强制关闭进程树」等待后端回执中 */
const treeClosing = ref(false)
/** 强制树杀没有在超时内确认收窗 */
const treeFailed = ref(false)
/** 不可关闭时持续时间内的剩余秒数 */
const remaining = ref(0)

let enterTimer: number | undefined
let failTimer: number | undefined
let treeFailTimer: number | undefined
let autoTimer: number | undefined

/**
 *
 * @param id
 */
function clearTimer(id: number | undefined): void {
  if (id !== undefined) window.clearTimeout(id)
}

/**
 *
 */
function stopAuto(): void {
  if (autoTimer !== undefined) {
    window.clearInterval(autoTimer)
    autoTimer = undefined
  }
}

/** 「不可关闭」的提醒没有按钮可点，只能靠这里的倒计时兜底收窗 */
function startAuto(): void {
  stopAuto()
  if (!alert.value?.block_close) {
    remaining.value = 0
    return
  }
  remaining.value = Math.max(1, alert.value.auto_close_secs)
  autoTimer = window.setInterval(() => {
    remaining.value -= 1
    if (remaining.value <= 0) {
      stopAuto()
      void dismiss()
    }
  }, 1000)
}

/**
 *
 */
function playEnter(): void {
  visible.value = false
  clearTimer(enterTimer)
  // 后端先 emit 再 show：等原生窗口真正显示出来再起动画，否则动画在看不见的时候播完了
  enterTimer = window.setTimeout(() => {
    visible.value = true
  }, 60)
}

/**
 *
 */
async function hide(): Promise<void> {
  try {
    await getCurrentWindow().hide()
  } catch {
    /* 窗口可能已销毁 */
  }
}

/**
 *
 */
async function playLeave(): Promise<void> {
  clearTimer(enterTimer)
  stopAuto()
  visible.value = false
  // 给 200ms 过渡留时间，再真正隐藏窗口
  await new Promise((resolve) => window.setTimeout(resolve, 180))
  await hide()
}

useTauriEvent<AlertPayload>(EV.ALERT, (payload) => {
  const sameRule = alert.value?.rule_id === payload.rule_id
  alert.value = payload
  closing.value = false
  closeFailed.value = false
  treeClosing.value = false
  treeFailed.value = false
  startAuto()
  // 同一规则的逐轮刷新（清理阶段每 N 秒重弹）：窗口已经开着就只换内容，不重播动画
  if (!sameRule || !visible.value) playEnter()
})

useTauriEvent<string>(EV.ALERT_CLOSED, () => {
  // 告警已被处置或用户确认：先播退场，再清数据收窗
  clearTimer(failTimer)
  clearTimer(treeFailTimer)
  stopAuto()
  closing.value = false
  treeClosing.value = false
  void playLeave().then(() => {
    alert.value = null
  })
})

// 后台每秒广播一次全量快照：用量继续累计；清理阶段的残留数也跟着每秒复核结果走。
// 只更新载荷字段，不重发 EV_ALERT —— 否则弹窗倒计时会被重置为满格。
useTauriEvent<Snapshot>(EV.TICK, (snapshot) => {
  if (!alert.value) return
  const rule = snapshot.rules.find((r) => r.rule_id === alert.value?.rule_id)
  if (!rule) return
  alert.value = {
    ...alert.value,
    used_ms: rule.used_ms,
    remaining: alert.value.phase === "cleanup" ? rule.pending_procs : alert.value.remaining,
  }
})

onMounted(async () => {
  try {
    const pong = await ping()
    if (pong !== "pong") console.error("提醒窗能力探针返回异常", pong)
  } catch (e) {
    console.error("提醒窗能力探针失败（capabilities 可能漏配 toast 窗）", e)
  }
  try {
    const s = await getSettings()
    applyTheme(s.theme)
  } catch {
    /* 保持默认 */
  }
  try {
    alert.value = await getActiveAlert()
    if (alert.value) {
      startAuto()
      playEnter()
    }
  } catch {
    /* 没有活动告警 */
  }
})

onBeforeUnmount(() => {
  clearTimer(enterTimer)
  clearTimer(failTimer)
  clearTimer(treeFailTimer)
  stopAuto()
})

/** 「我知道了」：交给后端收窗。清理阶段下轮仍会重弹，任务状态不变。 */
async function dismiss(): Promise<void> {
  if (alert.value) {
    try {
      await dismissAlert(alert.value.rule_id)
    } catch (e) {
      console.error("关闭提醒失败", e)
    }
  }
  await playLeave()
  alert.value = null
}

/**
 *
 */
async function snooze(): Promise<void> {
  if (!alert.value) return
  try {
    await snoozeRule(alert.value.rule_id, 300)
  } catch (e) {
    console.error("延期失败", e)
  }
  await playLeave()
}

/**
 * 「立即关闭」：后端真的去关这个进程。
 * 成功时后端会推 EV_ALERT_CLOSED 收窗；1.5 秒没等到就说明没关成，把原因摆出来。
 */
async function closeNow(): Promise<void> {
  if (!alert.value || closing.value) return
  closing.value = true
  closeFailed.value = false
  try {
    await enforceNow(alert.value.rule_id)
  } catch (e) {
    console.error("请求关闭失败", e)
  }
  clearTimer(failTimer)
  failTimer = window.setTimeout(() => {
    if (!closing.value) return
    closing.value = false
    closeFailed.value = true
  }, 1500)
}

/**
 * 「强制关闭（结束进程树）」：只在不允许保留后台的清理弹窗出现。
 * 展开进程树属于破坏性操作，等待后端回执的超时提示与普通关闭分开，避免按钮语义混淆。
 */
async function forceCloseNow(): Promise<void> {
  if (!alert.value || closing.value || treeClosing.value) return
  treeClosing.value = true
  treeFailed.value = false
  try {
    await forceKillTree(alert.value.rule_id)
  } catch (e) {
    console.error("请求强制关闭进程树失败", e)
  }
  clearTimer(treeFailTimer)
  treeFailTimer = window.setTimeout(() => {
    if (!treeClosing.value) return
    treeClosing.value = false
    treeFailed.value = true
  }, 1500)
}
</script>

<template>
  <!-- 全屏形态：窗口已被后端切到全屏，走沉浸式布局 -->
  <AlertFullscreen
    v-if="alert && alert.fullscreen"
    :alert="alert"
    :remaining="remaining"
    :closing="closing"
    :close-failed="closeFailed"
    :tree-closing="treeClosing"
    :tree-failed="treeFailed"
    @dismiss="dismiss"
    @snooze="snooze"
    @close-now="closeNow"
    @force-close-now="forceCloseNow"
  />

  <ToastCard
    v-else-if="alert"
    :alert="alert"
    :remaining="remaining"
    :closing="closing"
    :close-failed="closeFailed"
    :tree-closing="treeClosing"
    :tree-failed="treeFailed"
    :visible="visible"
    @dismiss="dismiss"
    @snooze="snooze"
    @close-now="closeNow"
    @force-close-now="forceCloseNow"
  />

  <div v-else class="text-muted-foreground flex h-full items-center justify-center text-xs">没有活动的提醒</div>
</template>
