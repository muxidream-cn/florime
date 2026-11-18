import type { ExceedAction, MatchKind, MonitorMode, NotifyConfig, Rule, RuleInput, RuleKind } from "@/types/models"

/** 规则编辑弹窗的表单模型：把「分钟」这类给人看的单位与后端字段分开 */
export interface RuleForm {
  id: string
  display_name: string
  pattern: string
  match_kind: MatchKind
  mode: MonitorMode
  limit_minutes: number
  grace_secs: number
  on_exceed: ExceedAction
  allow_background: boolean
  enabled: boolean
  paused: boolean
  kind: RuleKind
  /** 0 表示不提醒休息 */
  rest_minutes: number
  rest_auto_continue: boolean
  rest: NotifyConfig
  /** 超限时是否弹提醒窗 */
  exceed_on: boolean
  exceed: NotifyConfig
}

/**
 *
 */
export function defaultNotify(): NotifyConfig {
  return {
    message: "",
    fullscreen: false,
    block_close: false,
    block_shortcuts: false,
    auto_close_secs: 30,
  }
}

/**
 *
 */
export function blankRuleForm(): RuleForm {
  return {
    id: "",
    display_name: "",
    pattern: "",
    match_kind: "full_path",
    mode: "foreground",
    limit_minutes: 60,
    grace_secs: 60,
    on_exceed: "notify_only",
    allow_background: false,
    enabled: true,
    paused: false,
    kind: "slack",
    rest_minutes: 0,
    rest_auto_continue: true,
    rest: defaultNotify(),
    exceed_on: true,
    exceed: defaultNotify(),
  }
}

/**
 * 编辑回填：旧规则可能没有新字段，一律先铺默认值再覆盖
 * @param rule
 */
export function ruleToForm(rule: Rule): RuleForm {
  return {
    id: rule.id,
    display_name: rule.display_name,
    pattern: rule.pattern,
    match_kind: rule.match_kind,
    mode: rule.mode,
    limit_minutes: Math.max(1, Math.round(rule.daily_limit_secs / 60)),
    grace_secs: rule.grace_secs,
    on_exceed: rule.on_exceed,
    allow_background: rule.allow_background,
    enabled: rule.enabled,
    paused: rule.paused ?? false,
    kind: rule.kind ?? "slack",
    rest_minutes: rule.rest_remind_secs > 0 ? Math.max(5, Math.round(rule.rest_remind_secs / 60)) : 0,
    rest_auto_continue: rule.rest_auto_continue ?? true,
    rest: { ...defaultNotify(), ...(rule.rest ?? {}) },
    // 旧后端/旧缓存没有这个字段时按「开启」处理，保持原行为
    exceed_on: rule.exceed_remind ?? true,
    exceed: { ...defaultNotify(), ...(rule.exceed ?? {}) },
  }
}

/**
 *
 * @param form
 */
export function formToInput(form: RuleForm): RuleInput {
  return {
    id: form.id || undefined,
    display_name: form.display_name.trim(),
    pattern: form.pattern.trim(),
    match_kind: form.match_kind,
    mode: form.mode,
    daily_limit_secs: Math.round(form.limit_minutes * 60),
    grace_secs: Math.round(form.grace_secs),
    on_exceed: form.on_exceed,
    allow_background: form.allow_background,
    enabled: form.enabled,
    paused: form.paused,
    kind: form.kind,
    // 摸鱼模式不带休息提醒；专注模式下 0 表示用户关掉了
    rest_remind_secs: form.kind === "focus" && form.rest_minutes > 0 ? Math.round(form.rest_minutes * 60) : 0,
    rest_auto_continue: form.rest_auto_continue,
    rest: { ...form.rest },
    exceed_remind: form.exceed_on,
    exceed: { ...form.exceed },
  }
}

/**
 * 只做前端即时校验；落盘前 Rust 侧还会再收敛一次
 * @param form
 */
export function validateRuleForm(form: RuleForm): string {
  if (!form.display_name.trim()) return "请填名称"
  if (!form.pattern.trim()) return "请选目标进程或手填 exe 名"
  if (form.match_kind === "full_path" && !looksLikePath(form.pattern)) {
    return "完整路径需要包含目录，只填进程名请改用「仅进程名」"
  }
  if (form.limit_minutes <= 0) return "上限必须大于 0"
  if (form.grace_secs > form.limit_minutes * 60) return "宽限期不能超过每日上限"
  if (form.kind === "focus" && form.rest_minutes > 0) {
    if (form.rest_minutes < 5) return "休息提醒间隔至少 5 分钟"
    if (form.rest_minutes > 480) return "休息提醒间隔最多 480 分钟"
  }
  return ""
}

/**
 *
 * @param value
 */
export function looksLikePath(value: string): boolean {
  return value.includes("\\") || value.includes("/")
}
