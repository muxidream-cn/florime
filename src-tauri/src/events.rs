use serde::Serialize;

/// 前端订阅的事件名（唯一来源）。
pub const EV_TICK: &str = "focus://tick";
pub const EV_ALERT: &str = "focus://alert";
pub const EV_ALERT_CLOSED: &str = "focus://alert-closed";
pub const EV_STATE: &str = "focus://state";
/// 专注模式的休息提醒（独立窗口，不占用超限告警队列）
pub const EV_REST: &str = "focus://rest";
pub const EV_REST_CLOSED: &str = "focus://rest-closed";
/// 休息持续时间结束但未开启自动继续时，询问用户是否继续努力
pub const EV_REST_CONFIRM: &str = "focus://rest-confirm";
/// 屏幕拾取窗口：成功选到目标 / 被取消（右键、Esc、超时）
pub const EV_WINDOW_PICKED: &str = "focus://window-picked";
pub const EV_WINDOW_PICK_CANCELLED: &str = "focus://window-pick-cancelled";

/// 当前前台进程信息。
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProcInfo {
    pub pid: u32,
    pub exe_name: String,
    /// 小写完整路径；识别失败时是 `pid:<pid>`
    pub exe_key: String,
    /// 完整路径，识别失败为 None
    pub path: Option<String>,
    pub title: String,
    /// ok | unknown | denied
    pub integrity: String,
    /// 是否被某条规则命中
    pub tracked: bool,
}


/// 单条规则的实时用量。
#[derive(Debug, Clone, Serialize)]
pub struct RuleUsage {
    pub rule_id: String,
    pub display_name: String,
    pub used_ms: u64,
    pub limit_ms: u64,
    pub remaining_ms: u64,
    /// normal | warning | enforced | cleanup
    pub state: String,
    /// 清理阶段按规则仍命中的进程数（非清理阶段恒为 0）
    pub pending_procs: u32,
    /// 清理阶段下一轮复检的墙钟时间（毫秒；非清理阶段为 0）
    pub next_check_ts: i64,
    /// 宽限期结束的墙钟时间（毫秒；非宽限阶段为 0）
    pub grace_deadline_ts: i64,
    /// 本轮宽限期的总时长（毫秒；非宽限阶段为 0）
    pub grace_ms: u64,
}

/// 每秒推送给前端的快照。
#[derive(Debug, Clone, Serialize, Default)]
pub struct Snapshot {
    pub paused: bool,
    /// 休息提醒期间计时被冻结（与用户手动暂停区分）
    pub resting: bool,
    pub today_total_ms: u64,
    /// 当日仅专注规则命中的去重时长
    pub today_focus_ms: u64,
    /// 当日仅摸鱼规则命中的去重时长
    pub today_slack_ms: u64,
    /// 当日专注与摸鱼同时命中的去重时长
    pub today_focus_slack_ms: u64,
    pub current: Option<ProcInfo>,
    pub rules: Vec<RuleUsage>,
    /// 本轮是否有专注规则正在命中并计时
    pub focus_active: bool,
    /// 本轮是否有摸鱼规则正在命中并计时
    pub slack_active: bool,
    /// poll | event
    pub trigger_mode: String,
    /// 是否以管理员身份运行
    pub elevated: bool,
}

/// 超限告警载荷。推给 toast 窗口。
#[derive(Debug, Clone, Serialize)]
pub struct AlertPayload {
    pub alert_id: String,
    pub rule_id: String,
    pub display_name: String,
    pub exe_name: String,
    pub exe_key: String,
    pub pid: u32,
    pub used_ms: u64,
    pub limit_ms: u64,
    pub grace_ms: u64,
    /// 宽限期结束的墙钟时间（毫秒）
    pub grace_deadline_ts: i64,
    /// notify_only | graceful_close | force_kill
    pub action: String,
    /// 触发时是否已超限（false 表示只是提示）
    pub exceeded: bool,
    /// warn：宽限期内；cleanup：宽限已过、仍在等后台清理
    pub phase: String,
    /// 清理阶段按规则仍命中的进程数
    pub remaining: usize,
    /// 清理阶段下一轮复检/重弹的墙钟时间（毫秒；warn 阶段为 0）
    pub next_check_ts: i64,
    /// 清理阶段轮次（warn 阶段为 0）
    pub round: u32,
    /// 规则是否允许保留后台进程；false 时清理弹窗提供强制关闭进程树按钮
    pub allow_background: bool,
    /// 自定义文案；空串表示用默认文案
    pub message: String,
    /// 以下来自规则的超限提醒配置
    pub fullscreen: bool,
    pub block_close: bool,
    pub block_shortcuts: bool,
    pub auto_close_secs: u64,
}

/// 专注模式的休息提醒载荷。推给独立的 rest 窗口。
#[derive(Debug, Clone, Serialize)]
pub struct RestPayload {
    pub alert_id: String,
    pub rule_id: String,
    pub display_name: String,
    pub exe_name: String,
    /// 自定义文案；空串表示用默认文案
    pub message: String,
    pub fullscreen: bool,
    pub block_close: bool,
    pub block_shortcuts: bool,
    pub auto_close_secs: u64,
    /// 触发时的当日累计使用时长
    pub used_ms: u64,
    /// 休息提醒间隔（秒）
    pub interval_secs: u64,
    /// 休息结束后是否自动继续计时；false 时到点询问用户
    pub rest_auto_continue: bool,
}

/// 暂停/恢复状态变更推送。
#[derive(Debug, Clone, Serialize)]
pub struct PausedPayload {
    pub paused: bool,
}

pub fn paused_payload(paused: bool) -> PausedPayload {
    PausedPayload { paused }
}

/// 屏幕拾取到的窗口所属进程。前端据此填「目标 / 匹配方式 / 名称」。
#[derive(Debug, Clone, Serialize)]
pub struct PickedWindow {
    pub pid: u32,
    pub exe_name: String,
    /// 完整路径；识别失败为 None（此时只能按进程名建规则）
    pub path: Option<String>,
    pub title: String,
    /// 受保护进程：可以选中，但不能被处置
    pub is_protected: bool,
    /// 位于 Windows 目录下的系统自带程序
    pub is_system: bool,
}
