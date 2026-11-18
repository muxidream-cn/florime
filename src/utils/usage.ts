import { pct } from "@/utils/format"

/** 用量色档：低于 80% 绿、80% 起黄、到顶（含超额）红 */
export type UsageTone = "ok" | "warn" | "danger"

/**
 * 按「已用 / 上限」算色档。
 *
 * 只由百分比决定，不看 limit 状态机的阶段：清理/已处置时用量必然已经到顶，
 * 自然落到红色，不需要额外的状态覆盖。
 * @param used
 * @param limit
 */
export function usageTone(used: number, limit: number): UsageTone {
  if (limit <= 0) return "ok"
  const percent = pct(used, limit)
  if (percent >= 100) return "danger"
  return percent >= 80 ? "warn" : "ok"
}

const TEXT_CLASS: Record<UsageTone, string> = {
  ok: "text-emerald-600 dark:text-emerald-400",
  warn: "text-amber-600 dark:text-amber-400",
  danger: "text-destructive",
}

const BAR_CLASS: Record<UsageTone, string> = {
  ok: "bg-emerald-500",
  warn: "bg-amber-500",
  danger: "bg-destructive",
}

const RING_TONE: Record<UsageTone, "normal" | "warning" | "danger"> = {
  ok: "normal",
  warn: "warning",
  danger: "danger",
}

/**
 * 时间文字颜色
 * @param tone
 */
export function usageTextClass(tone: UsageTone): string {
  return TEXT_CLASS[tone]
}

/**
 * 进度条填充色
 * @param tone
 */
export function usageBarClass(tone: UsageTone): string {
  return BAR_CLASS[tone]
}

/**
 * 圆环组件的 tone 参数（UsageRing / GraceCountdown 通用）
 * @param tone
 */
export function usageRingTone(tone: UsageTone): "normal" | "warning" | "danger" {
  return RING_TONE[tone]
}

/**
 * 概览卡片上的一条倒计时时间线。
 *
 * rest/limit 由「当日用量 + 间隔 + 额度」推导；grace 来自后端下发的宽限截止时间。
 * 只有 1Hz 快照刷新时倒计时才动；暂停或休息冻结时自然停住。
 */
export interface RuleTimelineItem {
  /** rest：距下轮休息；limit：距额度用尽；grace：宽限倒计时；over：已超出的时长 */
  id: "rest" | "limit" | "grace" | "over"
  label: string
  remainingMs: number
  totalMs: number
  /** 进度条填充百分比（0–100） */
  percent: number
  /** focus 是休息线专用色，其余跟随用量色档 */
  tone: UsageTone | "focus"
}
