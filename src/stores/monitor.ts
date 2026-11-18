import { defineStore } from "pinia"
import { computed, ref } from "vue"

import * as api from "@/api/commands"

import type { ProcInfo, Snapshot } from "@/types/models"

/**
 *
 */
function emptySnapshot(): Snapshot {
  return {
    paused: false,
    resting: false,
    today_total_ms: 0,
    today_focus_ms: 0,
    today_slack_ms: 0,
    today_focus_slack_ms: 0,
    current: null,
    rules: [],
    focus_active: false,
    slack_active: false,
    trigger_mode: "poll",
    elevated: false,
  }
}

export const useMonitorStore = defineStore("monitor", () => {
  const snapshot = ref<Snapshot>(emptySnapshot())
  const connected = ref(false)
  const lastTickAt = ref(0)
  /** 能力探针结果：capabilities 漏配时 invoke/listen 会静默失败，只能靠主动探测发现 */
  const health = ref<"unknown" | "ok" | "error">("unknown")

  const paused = computed(() => snapshot.value.paused)
  /** 休息提醒展示中：计时被冻结，但和用户手动暂停不是一回事 */
  const resting = computed(() => snapshot.value.resting)
  const current = computed<ProcInfo | null>(() => snapshot.value.current)
  /** 今日累计（按时间轴去重），不是各规则用量之和 */
  const todayTotalMs = computed(() => snapshot.value.today_total_ms)
  /** 三类互斥：仅专注 / 仅摸鱼 / 专注且摸鱼，相加等于 todayTotalMs */
  const focusTotalMs = computed(() => snapshot.value.today_focus_ms)
  const slackTotalMs = computed(() => snapshot.value.today_slack_ms)
  const focusSlackTotalMs = computed(() => snapshot.value.today_focus_slack_ms)
  const ruleUsages = computed(() => snapshot.value.rules)
  /** 本轮快照中正在命中并计时的规则类型 */
  const focusActive = computed(() => snapshot.value.focus_active)
  const slackActive = computed(() => snapshot.value.slack_active)
  const elevated = computed(() => snapshot.value.elevated)

  /** 1 秒没收到 tick 就认为监控链断了（正常应该每秒都有） */
  const stale = computed(() => connected.value && Date.now() - lastTickAt.value > 3000)

  /**
   *
   * @param next
   */
  function setSnapshot(next: Snapshot): void {
    snapshot.value = next
    lastTickAt.value = Date.now()
  }

  /**
   *
   * @param value
   */
  function setPaused(value: boolean): void {
    snapshot.value = { ...snapshot.value, paused: value }
  }

  /**
   *
   */
  async function bootstrap(): Promise<void> {
    snapshot.value = await api.getSnapshot()
    connected.value = true
    lastTickAt.value = Date.now()
  }

  /** 每个窗口启动各打一次，失败即代表本窗的 IPC 权限有问题。 */
  async function checkCapabilities(): Promise<boolean> {
    try {
      health.value = (await api.ping()) === "pong" ? "ok" : "error"
    } catch (e) {
      console.error("能力探针失败", e)
      health.value = "error"
    }
    return health.value === "ok"
  }

  /**
   *
   */
  async function togglePause(): Promise<void> {
    await api.pauseMonitor(!snapshot.value.paused)
    setPaused(!snapshot.value.paused)
  }

  /**
   *
   * @param ruleId
   */
  function usageOf(ruleId: string) {
    return snapshot.value.rules.find((r) => r.rule_id === ruleId)
  }

  return {
    snapshot,
    connected,
    health,
    paused,
    resting,
    current,
    todayTotalMs,
    focusTotalMs,
    slackTotalMs,
    focusSlackTotalMs,
    ruleUsages,
    focusActive,
    slackActive,
    elevated,
    stale,
    setSnapshot,
    setPaused,
    bootstrap,
    checkCapabilities,
    togglePause,
    usageOf,
  }
})
