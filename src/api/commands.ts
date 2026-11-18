import { invoke } from "@tauri-apps/api/core"

import type {
  AlertPayload,
  AppMeta,
  DayCategoryStat,
  DayStat,
  ExeStat,
  ProcRow,
  Rule,
  RuleInput,
  Settings,
  SettingsPatch,
  Snapshot,
} from "@/types/models"

/** 能力探针：确认窗口的 capabilities 配对正确。 */
export const ping = () => invoke<string>("ping")

export const getSnapshot = () => invoke<Snapshot>("get_snapshot")
export const pauseMonitor = (paused: boolean) => invoke<void>("pause_monitor", { paused })
export const snoozeRule = (ruleId: string, secs: number) => invoke<void>("snooze_rule", { ruleId, secs })
export const enforceNow = (ruleId: string) => invoke<void>("enforce_now", { ruleId })
/**
 * 「强制关闭（结束进程树）」：仅不允许保留后台的清理弹窗可用
 * @param ruleId
 */
export const forceKillTree = (ruleId: string) => invoke<void>("force_kill_tree", { ruleId })
export const getActiveAlert = () => invoke<AlertPayload | null>("get_active_alert")
/**
 * 用户点「我知道了」：收提醒窗。
 *
 * 宽限期内只是不再打扰（宽限照常走）；「等待后台清理」阶段则只收窗，
 * 任务状态保留在仪表盘，下一轮复检会重新弹出，直到残留进程消失。
 * @param ruleId
 */
export const dismissAlert = (ruleId: string) => invoke<void>("dismiss_alert", { ruleId })

/** 休息提醒窗的就绪探针：窗口首次创建时补弹挂起的载荷。 */
export const restReady = () => invoke<void>("rest_ready")

/** 收起休息提醒并恢复计时（前端倒计时结束 / 用户主动关闭）。 */
export const hideRest = () => invoke<void>("hide_rest")

/** 进入屏幕拾取模式：点击任意应用窗口即选中其进程（15 秒超时）。 */
export const startWindowPick = () => invoke<void>("start_window_pick")

/** 取消屏幕拾取（关闭弹窗 / 组件卸载时的兜底）。 */
export const cancelWindowPick = () => invoke<void>("cancel_window_pick")

export const getRules = () => invoke<Rule[]>("get_rules")
export const upsertRule = (input: RuleInput) => invoke<Rule[]>("upsert_rule", { input })
export const deleteRule = (ruleId: string) => invoke<Rule[]>("delete_rule", { ruleId })
export const setRuleEnabled = (ruleId: string, enabled: boolean) => invoke<Rule[]>("set_rule_enabled", { ruleId, enabled })
export const setRulePaused = (ruleId: string, paused: boolean) => invoke<Rule[]>("set_rule_paused", { ruleId, paused })
/**
 * 保存拖拽后的规则顺序；返回重排后的完整列表
 * @param ids
 */
export const reorderRules = (ids: string[]) => invoke<Rule[]>("reorder_rules", { ids })

export const getSettings = () => invoke<Settings>("get_settings")
export const updateSettings = (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch })
export const isAutostartEnabled = () => invoke<boolean>("is_autostart_enabled")

export interface ProcessListOptions {
  /** 默认 true：过滤掉受保护进程（系统关键 / Windows 目录 / 自身链 / 黑名单） */
  excludeProtected?: boolean
  /** 默认 true：过滤掉 Windows 目录下的系统程序 */
  ignoreSystem?: boolean
}

export const listRunningProcesses = (opts: ProcessListOptions = {}) =>
  invoke<ProcRow[]>("list_running_processes", {
    excludeProtected: opts.excludeProtected ?? true,
    ignoreSystem: opts.ignoreSystem ?? true,
  })

/**
 * 批量读取程序图标与应用名。
 *
 * 入参既可以是完整路径，也可以是纯 exe 名；返回以入参为 key 的映射。
 * @param values
 */
export const getAppMeta = (values: string[]) => invoke<Record<string, AppMeta>>("get_app_meta", { values })

export const queryDayStats = (from: string, to: string) => invoke<DayStat[]>("query_day_stats", { from, to })
export const queryDayDetail = (day: string) => invoke<ExeStat[]>("query_day_detail", { day })
export const queryTopExes = (from: string, to: string, limit = 8) => invoke<ExeStat[]>("query_top_exes", { from, to, limit })
export const queryGrandTotal = () => invoke<number>("query_grand_total")
export const queryDayCategoryStats = (from: string, to: string) => invoke<DayCategoryStat[]>("query_day_category_stats", { from, to })
export const exportCsv = (from: string, to: string) => invoke<string>("export_csv", { from, to })

export const openDataDir = () => invoke<string>("open_data_dir")
/**
 * 在资源管理器中定位规则对应程序所在目录
 * @param pattern
 */
export const revealRulePath = (pattern: string) => invoke<void>("reveal_rule_path", { pattern })
export const dbPath = () => invoke<string>("db_path")
export const isElevated = () => invoke<boolean>("is_elevated")
export const restartAsAdmin = () => invoke<void>("restart_as_admin")
