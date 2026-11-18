/**
 * 毫秒 → "3 小时 12 分" / "12 分 05 秒"
 * @param ms
 * @param opts
 * @param opts.compact
 */
export function formatDuration(ms: number, opts: { compact?: boolean } = {}): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60

  if (h > 0) return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`
  if (opts.compact) return `${m} 分钟`
  if (m > 0) return `${m} 分 ${String(s).padStart(2, "0")} 秒`
  return `${s} 秒`
}

/**
 * 毫秒 → "12:05" 倒计时样式
 * @param ms
 */
export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
}

/**
 *
 * @param ms
 */
export function formatMinutes(ms: number): string {
  return `${Math.floor(Math.max(0, ms) / 60_000)} 分钟`
}

/**
 * 秒 → 人类可读
 * @param secs
 */
export function formatSecs(secs: number): string {
  if (secs < 60) return `${secs} 秒`
  if (secs % 3600 === 0) return `${secs / 3600} 小时`
  if (secs % 60 === 0) return `${secs / 60} 分钟`
  return formatDuration(secs * 1000)
}

/**
 * 从路径取文件名
 * @param path
 */
export function baseName(path: string | null | undefined): string {
  if (!path) return ""
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] ?? ""
}

/**
 * 日期 → YYYY-MM-DD（本地时区，不用 toISOString 以免时区偏移）
 * @param d
 */
export function toDayString(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

/**
 *
 */
export function todayString(): string {
  return toDayString(new Date())
}

/**
 * 往前推 n 天的日期字符串
 * @param n
 */
export function daysAgo(n: number): string {
  const d = new Date()
  d.setDate(d.getDate() - n)
  return toDayString(d)
}

/**
 *
 * @param used
 * @param limit
 */
export function pct(used: number, limit: number): number {
  if (limit <= 0) return 0
  return Math.min(100, Math.max(0, (used / limit) * 100))
}
