// 与 Rust 侧 events.rs / config/model.rs 严格对齐的类型定义。

export type MonitorMode = "foreground" | "running"
export type MatchKind = "full_path" | "exe_name"
export type ExceedAction = "notify_only" | "graceful_close" | "force_kill"
export type TriggerMode = "poll" | "event"
export type CloseBehavior = "minimize_to_tray" | "quit"
export type LimitResetMode = "natural_day" | "since_launch"
export type ThemeMode = "system" | "light" | "dark"
export type Integrity = "ok" | "unknown" | "denied"

/** 规则模式：专注会按当日累计时长定期提醒休息；摸鱼只管额度与超限处置 */
export type RuleKind = "focus" | "slack"

/**
 * 提醒的呈现配置：休息提醒与超限提醒共用同一套字段。
 *
 * 依赖关系由 Rust 侧 `normalize` 收敛：非全屏不许「不可关闭」，
 * 非「不可关闭」不许「屏蔽快捷键」。
 */
export interface NotifyConfig {
  /** 自定义文案；空串表示用默认文案 */
  message: string
  /** 是否全屏提醒 */
  fullscreen: boolean
  /** 是否禁止关闭（仅全屏时有效） */
  block_close: boolean
  /** 是否屏蔽切出快捷键（仅全屏 + 不可关闭时有效） */
  block_shortcuts: boolean
  /** 自动消失秒数（5–300） */
  auto_close_secs: number
}

export interface ProcInfo {
  pid: number
  exe_name: string
  exe_key: string
  path: string | null
  title: string
  integrity: Integrity
  tracked: boolean
}

export type RuleUsageState = "normal" | "warning" | "enforced" | "cleanup"

export interface RuleUsage {
  rule_id: string
  display_name: string
  used_ms: number
  limit_ms: number
  remaining_ms: number
  state: RuleUsageState
  /** 清理阶段按规则仍命中的进程数（非清理阶段为 0） */
  pending_procs: number
  /** 清理阶段下一轮复检的墙钟时间（毫秒；非清理阶段为 0） */
  next_check_ts: number
  /** 宽限期结束的墙钟时间（毫秒；非宽限阶段为 0） */
  grace_deadline_ts: number
  /** 本轮宽限期的总时长（毫秒；非宽限阶段为 0） */
  grace_ms: number
}

export interface Snapshot {
  paused: boolean
  /** 休息提醒期间计时被冻结（与用户手动暂停区分） */
  resting: boolean
  today_total_ms: number
  /** 当日仅专注规则命中的去重时长 */
  today_focus_ms: number
  /** 当日仅摸鱼规则命中的去重时长 */
  today_slack_ms: number
  /** 当日专注与摸鱼同时命中的去重时长 */
  today_focus_slack_ms: number
  current: ProcInfo | null
  rules: RuleUsage[]
  /** 本轮是否有专注规则正在命中并计时 */
  focus_active: boolean
  /** 本轮是否有摸鱼规则正在命中并计时 */
  slack_active: boolean
  trigger_mode: TriggerMode
  elevated: boolean
}

export interface Rule {
  id: string
  display_name: string
  match_kind: MatchKind
  pattern: string
  mode: MonitorMode
  daily_limit_secs: number
  grace_secs: number
  on_exceed: ExceedAction
  /** 允许保留后台进程：关闭前台后仍有残留时不再弹窗 */
  allow_background: boolean
  enabled: boolean
  /** 启用但暂停：不计时、不推进额度、不触发休息 */
  paused: boolean
  /** 专注 / 摸鱼；历史规则缺省为摸鱼 */
  kind: RuleKind
  /** 专注模式：每隔多少秒提醒一次休息；0 表示不提醒 */
  rest_remind_secs: number
  /** 休息结束后是否自动继续计时；旧规则缺省 true */
  rest_auto_continue: boolean
  /** 专注模式的休息提醒配置 */
  rest: NotifyConfig
  /** 超限时是否弹提醒窗；关闭后仍按处置执行，只是不提醒 */
  exceed_remind: boolean
  /** 两种模式共用的超限提醒配置 */
  exceed: NotifyConfig
}

export type RuleInput = Omit<Rule, "id"> & { id?: string }

export interface Settings {
  trigger_mode: TriggerMode
  poll_ms: number
  heartbeat_ms: number
  process_scan_ms: number
  /** 清理失败提醒间隔（秒）：仍有残留进程时的复检与重弹节拍 */
  cleanup_remind_secs: number
  close_behavior: CloseBehavior
  autostart: boolean
  /** 默认以管理员启动：每次启动非管理员时请求一次 UAC 提权 */
  launch_as_admin: boolean
  /** 记录所有前台程序的使用时长（设备使用统计），暂停和休息提醒期间不记录 */
  track_all_foreground: boolean
  idle_pause_enabled: boolean
  idle_pause_secs: number
  notify: boolean
  sound: boolean
  theme: ThemeMode
  limit_reset_mode: LimitResetMode
  whitelist: string[]
  blacklist: string[]
  extra_protected: string[]
  retention_days: number
}

export type SettingsPatch = Partial<Settings>

/** 程序展示元信息（按路径或 exe 名索引） */
export interface AppMeta {
  /** 应用显示名（exe 文件说明，取不到回退文件名） */
  name: string
  /** PNG data URL */
  icon: string | null
}

export type AppMetaMap = Record<string, AppMeta>

export interface AlertPayload {
  alert_id: string
  rule_id: string
  display_name: string
  exe_name: string
  exe_key: string
  pid: number
  used_ms: number
  limit_ms: number
  grace_ms: number
  grace_deadline_ts: number
  action: ExceedAction
  exceeded: boolean
  /** warn：宽限期内；cleanup：宽限已过、仍在等后台清理 */
  phase: "warn" | "cleanup"
  /** 清理阶段按规则仍命中的进程数 */
  remaining: number
  /** 清理阶段下一轮复检/重弹的墙钟时间（毫秒；warn 阶段为 0） */
  next_check_ts: number
  /** 清理阶段轮次（warn 阶段为 0） */
  round: number
  /** 规则是否允许保留后台进程；false 时清理弹窗提供强制关闭进程树按钮 */
  allow_background: boolean
  /** 自定义文案；空串表示用默认文案 */
  message: string
  fullscreen: boolean
  block_close: boolean
  block_shortcuts: boolean
  auto_close_secs: number
}

/** 专注模式的休息提醒载荷（独立窗口，不占超限告警队列） */
export interface RestPayload {
  alert_id: string
  rule_id: string
  display_name: string
  exe_name: string
  /** 自定义文案；空串表示用默认文案 */
  message: string
  fullscreen: boolean
  block_close: boolean
  block_shortcuts: boolean
  auto_close_secs: number
  /** 触发时的当日累计使用时长 */
  used_ms: number
  /** 休息提醒间隔（秒） */
  interval_secs: number
  /** 休息结束后是否自动继续计时；false 时到点询问用户 */
  rest_auto_continue: boolean
}

export interface ProcRow {
  pid: number
  exe_name: string
  path: string | null
  window_title: string
  instances: number
  /** 受保护进程：可以展示，但不能被配成处置目标 */
  is_protected: boolean
  /** 位于 Windows 目录下的系统自带程序 */
  is_system: boolean
}

/** 屏幕拾取到的窗口所属进程 */
export interface PickedWindow {
  pid: number
  exe_name: string
  /** 完整路径；识别失败为 null（只能按进程名建规则） */
  path: string | null
  title: string
  is_protected: boolean
  is_system: boolean
}

export interface DayStat {
  day: string
  total_ms: number
}

/** 每日分类统计：三类互斥，相加等于去重后的当日总时长 */
export interface DayCategoryStat {
  day: string
  focus_ms: number
  slack_ms: number
  focus_slack_ms: number
  total_ms: number
}

export interface ExeStat {
  exe_key: string
  exe_name: string
  total_ms: number
}

export interface AppErrorPayload {
  kind: string
  message: string
}
