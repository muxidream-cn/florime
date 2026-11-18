use serde::{Deserialize, Serialize};

use crate::config::normalize_exe_key;

/// 监控模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorMode {
    /// 只有该进程占据前台时计时
    Foreground,
    /// 进程存在即计时
    Running,
}

impl MonitorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Running => "running",
        }
    }
}

/// 规则匹配方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// 完整路径（推荐，避免同名 exe 误伤）
    FullPath,
    /// 仅可执行文件名
    ExeName,
}

/// 超限后的处置动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceedAction {
    /// 无操作：不做任何处置（提醒是否弹出由 exceed_remind 单独决定）
    NotifyOnly,
    /// 向前台窗口发送 WM_CLOSE，让它自己走正常关闭流程
    GracefulClose,
    /// TerminateProcess 强制结束
    ForceKill,
}

/// 前台检测的触发方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    /// 固定周期轮询
    Poll,
    /// WinEvent 事件驱动 + 心跳兜底
    Event,
}

/// 关闭主窗口的行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    MinimizeToTray,
    Quit,
}

/// 每日额度重置语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitResetMode {
    /// 本地时区 00:00 归零
    NaturalDay,
    /// 应用启动后累计，不随跨天归零
    SinceLaunch,
}

/// 主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

/// 规则模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// 专注：按当日累计使用时长定期提醒休息
    Focus,
    /// 摸鱼：原有额度 + 超限处置；缺省值，历史规则全部落在这里
    #[default]
    Slack,
}

fn default_auto_close() -> u64 {
    30
}

/// 提醒的呈现配置：休息提醒与超限提醒共用同一套字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    /// 自定义文案；空串表示用默认文案
    #[serde(default)]
    pub message: String,
    /// 是否全屏提醒
    #[serde(default)]
    pub fullscreen: bool,
    /// 是否禁止关闭（仅全屏时有效）
    #[serde(default)]
    pub block_close: bool,
    /// 是否屏蔽切出快捷键（仅全屏 + 不可关闭时有效）
    #[serde(default)]
    pub block_shortcuts: bool,
    /// 不可关闭时的自动消失秒数
    #[serde(default = "default_auto_close")]
    pub auto_close_secs: u64,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            message: String::new(),
            fullscreen: false,
            block_close: false,
            block_shortcuts: false,
            auto_close_secs: default_auto_close(),
        }
    }
}

impl NotifyConfig {
    /// 收敛非法组合：只有全屏才谈得上不可关闭，只有不可关闭才谈得上屏蔽快捷键。
    pub fn normalize(&mut self) {
        self.auto_close_secs = self.auto_close_secs.clamp(5, 300);
        self.message = self.message.trim().to_string();
        if !self.fullscreen {
            self.block_close = false;
        }
        if !self.block_close {
            self.block_shortcuts = false;
        }
    }
}

fn default_grace() -> u64 {
    60
}

fn default_poll_ms() -> u64 {
    1000
}

fn default_heartbeat_ms() -> u64 {
    500
}

fn default_process_scan_ms() -> u64 {
    2000
}

/// 清理失败提醒间隔（秒）：宽限期满后仍有命中进程时的复检/重弹节拍。
fn default_cleanup_remind() -> u64 {
    15
}

fn default_retention_days() -> u32 {
    180
}

fn default_idle_secs() -> u64 {
    300
}

/// 一条监控规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub display_name: String,
    pub match_kind: MatchKind,
    /// 已规范化的匹配值（小写；路径统一反斜杠）
    pub pattern: String,
    pub mode: MonitorMode,
    pub daily_limit_secs: u64,
    #[serde(default = "default_grace")]
    pub grace_secs: u64,
    pub on_exceed: ExceedAction,
    /// 是否允许保留后台进程；旧规则缺省 false，保持原有「有残留即提醒」行为
    #[serde(default)]
    pub allow_background: bool,
    #[serde(default = "crate::config::model::default_true")]
    pub enabled: bool,
    /// 启用但暂时停止计时；与「已关闭」区分，方便首页一键恢复
    #[serde(default)]
    pub paused: bool,
    /// 专注 / 摸鱼；历史规则没有这个字段，默认摸鱼
    #[serde(default)]
    pub kind: RuleKind,
    /// 专注模式：每隔多少秒提醒一次休息；0 表示不提醒
    #[serde(default)]
    pub rest_remind_secs: u64,
    /// 休息结束后是否自动继续计时；旧规则缺失时按 true 兼容
    #[serde(default = "crate::config::model::default_true")]
    pub rest_auto_continue: bool,
    /// 专注模式的休息提醒配置
    #[serde(default)]
    pub rest: NotifyConfig,
    /// 超限时是否弹提醒窗；关闭后仍按「超限后」处置执行，只是不弹窗、不通知
    #[serde(default = "crate::config::model::default_true")]
    pub exceed_remind: bool,
    /// 两种模式共用的超限提醒配置
    #[serde(default)]
    pub exceed: NotifyConfig,
}

pub fn default_true() -> bool {
    true
}

impl Rule {
    /// 规则是否命中某个进程。
    ///
    /// `path` 为 `None` 时（进程识别被拒绝），只允许 `ExeName` 规则生效，
    /// 避免同名不同路径的进程被误伤。
    pub fn matches(&self, exe_key: &str, exe_name: &str, has_path: bool) -> bool {
        match self.match_kind {
            MatchKind::FullPath => has_path && self.pattern == exe_key,
            MatchKind::ExeName => self.pattern == exe_name.to_lowercase(),
        }
    }

    /// 落盘前的收敛：休息间隔与自动消失时长夹到合理区间，清掉互相矛盾的开关键。
    pub fn normalize(&mut self) {
        if self.rest_remind_secs != 0 {
            // 最短 5 分钟、最长 8 小时，避免误填 1 秒造成刷屏
            self.rest_remind_secs = self.rest_remind_secs.clamp(300, 28_800);
        }
        self.rest.normalize();
        self.exceed.normalize();
    }
}

/// 规则集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleBook {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl RuleBook {
}

/// 全局设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_trigger")]
    pub trigger_mode: TriggerMode,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
    #[serde(default = "default_heartbeat_ms")]
    pub heartbeat_ms: u64,
    #[serde(default = "default_process_scan_ms")]
    pub process_scan_ms: u64,
    /// 清理失败提醒间隔（秒）：处置后仍有命中进程时，每轮重弹提醒的间隔
    #[serde(default = "default_cleanup_remind")]
    pub cleanup_remind_secs: u64,
    #[serde(default = "default_close_behavior")]
    pub close_behavior: CloseBehavior,
    #[serde(default)]
    pub autostart: bool,
    /// 默认以管理员启动：每次启动检测到非管理员时请求一次 UAC 提权
    #[serde(default)]
    pub launch_as_admin: bool,
    /// 记录所有前台程序的使用时长（设备使用统计），暂停和休息提醒期间不记录
    #[serde(default = "default_true")]
    pub track_all_foreground: bool,
    #[serde(default)]
    pub idle_pause_enabled: bool,
    #[serde(default = "default_idle_secs")]
    pub idle_pause_secs: u64,
    #[serde(default = "crate::config::model::default_true")]
    pub notify: bool,
    #[serde(default = "crate::config::model::default_true")]
    pub sound: bool,
    #[serde(default = "default_theme")]
    pub theme: ThemeMode,
    #[serde(default = "default_reset_mode")]
    pub limit_reset_mode: LimitResetMode,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub blacklist: Vec<String>,
    #[serde(default)]
    pub extra_protected: Vec<String>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

fn default_trigger() -> TriggerMode {
    TriggerMode::Poll
}

fn default_close_behavior() -> CloseBehavior {
    CloseBehavior::MinimizeToTray
}

fn default_theme() -> ThemeMode {
    ThemeMode::System
}

fn default_reset_mode() -> LimitResetMode {
    LimitResetMode::NaturalDay
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            trigger_mode: default_trigger(),
            poll_ms: default_poll_ms(),
            heartbeat_ms: default_heartbeat_ms(),
            process_scan_ms: default_process_scan_ms(),
            cleanup_remind_secs: default_cleanup_remind(),
            close_behavior: default_close_behavior(),
            autostart: false,
            launch_as_admin: false,
            track_all_foreground: true,
            idle_pause_enabled: false,
            idle_pause_secs: default_idle_secs(),
            notify: true,
            sound: true,
            theme: default_theme(),
            limit_reset_mode: default_reset_mode(),
            whitelist: Vec::new(),
            blacklist: Vec::new(),
            extra_protected: Vec::new(),
            retention_days: default_retention_days(),
        }
    }
}

impl Settings {
    /// 设置项被外部（前端 patch）修改后做一次收敛，避免非法值进入监控线程。
    pub fn sanitize(&mut self) {
        self.poll_ms = self.poll_ms.clamp(500, 10_000);
        self.heartbeat_ms = self.heartbeat_ms.clamp(200, 5_000);
        self.process_scan_ms = self.process_scan_ms.clamp(1000, 30_000);
        self.cleanup_remind_secs = self.cleanup_remind_secs.clamp(5, 60);
        self.idle_pause_secs = self.idle_pause_secs.clamp(60, 7200);
        self.retention_days = self.retention_days.clamp(7, 3650);
        self.whitelist = self.whitelist.iter().map(|s| normalize_exe_key(s)).collect();
        self.blacklist = self.blacklist.iter().map(|s| normalize_exe_key(s)).collect();
        self.extra_protected = self
            .extra_protected
            .iter()
            .map(|s| normalize_exe_key(s))
            .collect();
    }
}

/// 前端提交的规则输入（id 为空表示新建）。
#[derive(Debug, Clone, Deserialize)]
pub struct RuleInput {
    #[serde(default)]
    pub id: String,
    pub display_name: String,
    pub match_kind: MatchKind,
    pub pattern: String,
    pub mode: MonitorMode,
    pub daily_limit_secs: u64,
    #[serde(default = "default_grace")]
    pub grace_secs: u64,
    pub on_exceed: ExceedAction,
    /// 是否允许保留后台进程；旧规则缺省 false，保持原有「有残留即提醒」行为
    #[serde(default)]
    pub allow_background: bool,
    #[serde(default = "crate::config::model::default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub kind: RuleKind,
    #[serde(default)]
    pub rest_remind_secs: u64,
    #[serde(default = "crate::config::model::default_true")]
    pub rest_auto_continue: bool,
    #[serde(default)]
    pub rest: NotifyConfig,
    #[serde(default = "crate::config::model::default_true")]
    pub exceed_remind: bool,
    #[serde(default)]
    pub exceed: NotifyConfig,
}

/// 设置的部分更新（前端只传改动字段）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub trigger_mode: Option<TriggerMode>,
    pub poll_ms: Option<u64>,
    pub heartbeat_ms: Option<u64>,
    pub process_scan_ms: Option<u64>,
    pub cleanup_remind_secs: Option<u64>,
    pub close_behavior: Option<CloseBehavior>,
    pub autostart: Option<bool>,
    pub launch_as_admin: Option<bool>,
    pub track_all_foreground: Option<bool>,
    pub idle_pause_enabled: Option<bool>,
    pub idle_pause_secs: Option<u64>,
    pub notify: Option<bool>,
    pub sound: Option<bool>,
    pub theme: Option<ThemeMode>,
    pub limit_reset_mode: Option<LimitResetMode>,
    pub whitelist: Option<Vec<String>>,
    pub blacklist: Option<Vec<String>>,
    pub extra_protected: Option<Vec<String>>,
    pub retention_days: Option<u32>,
}

impl SettingsPatch {
    pub fn apply_to(self, base: &mut Settings) {
        if let Some(v) = self.trigger_mode {
            base.trigger_mode = v;
        }
        if let Some(v) = self.poll_ms {
            base.poll_ms = v;
        }
        if let Some(v) = self.heartbeat_ms {
            base.heartbeat_ms = v;
        }
        if let Some(v) = self.process_scan_ms {
            base.process_scan_ms = v;
        }
        if let Some(v) = self.cleanup_remind_secs {
            base.cleanup_remind_secs = v;
        }
        if let Some(v) = self.close_behavior {
            base.close_behavior = v;
        }
        if let Some(v) = self.autostart {
            base.autostart = v;
        }
        if let Some(v) = self.launch_as_admin {
            base.launch_as_admin = v;
        }
        if let Some(v) = self.track_all_foreground {
            base.track_all_foreground = v;
        }
        if let Some(v) = self.idle_pause_enabled {
            base.idle_pause_enabled = v;
        }
        if let Some(v) = self.idle_pause_secs {
            base.idle_pause_secs = v;
        }
        if let Some(v) = self.notify {
            base.notify = v;
        }
        if let Some(v) = self.sound {
            base.sound = v;
        }
        if let Some(v) = self.theme {
            base.theme = v;
        }
        if let Some(v) = self.limit_reset_mode {
            base.limit_reset_mode = v;
        }
        if let Some(v) = self.whitelist {
            base.whitelist = v;
        }
        if let Some(v) = self.blacklist {
            base.blacklist = v;
        }
        if let Some(v) = self.extra_protected {
            base.extra_protected = v;
        }
        if let Some(v) = self.retention_days {
            base.retention_days = v;
        }
        base.sanitize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_rule_json() -> serde_json::Value {
        serde_json::json!({
            "id": "r1",
            "display_name": "旧规则",
            "match_kind": "exe_name",
            "pattern": "game.exe",
            "mode": "foreground",
            "daily_limit_secs": 3600,
            "on_exceed": "force_kill"
        })
    }

    #[test]
    fn legacy_rule_defaults_to_slack_without_rest() {
        let rule: Rule = serde_json::from_value(base_rule_json()).expect("旧规则必须能反序列化");
        assert_eq!(rule.kind, RuleKind::Slack);
        assert_eq!(rule.rest_remind_secs, 0);
        assert_eq!(rule.rest.auto_close_secs, 30);
        assert!(!rule.rest.fullscreen);
        assert!(!rule.exceed.block_close);
    }

    #[test]
    fn legacy_kill_tree_is_ignored_and_background_not_allowed() {
        let mut json = base_rule_json();
        json["kill_tree"] = serde_json::json!(true);
        let rule: Rule = serde_json::from_value(json).expect("旧规则的未知字段必须可忽略");
        assert!(
            !rule.allow_background,
            "旧 kill_tree 不应被当成允许保留后台，避免升级后静默放行"
        );
    }

    #[test]
    fn normalize_clears_dependent_toggles() {
        let mut cfg = NotifyConfig {
            fullscreen: false,
            block_close: true,
            block_shortcuts: true,
            auto_close_secs: 1,
            ..NotifyConfig::default()
        };
        cfg.normalize();
        assert!(!cfg.block_close, "非全屏不该保留不可关闭");
        assert!(!cfg.block_shortcuts, "非全屏不该保留屏蔽快捷键");
        assert_eq!(cfg.auto_close_secs, 5, "自动消失时长下限 5 秒");

        let mut cfg = NotifyConfig {
            fullscreen: true,
            block_close: false,
            block_shortcuts: true,
            auto_close_secs: 9999,
            ..NotifyConfig::default()
        };
        cfg.normalize();
        assert!(!cfg.block_shortcuts, "允许关闭时屏蔽快捷键无意义");
        assert_eq!(cfg.auto_close_secs, 300, "自动消失时长上限 300 秒");
    }

    #[test]
    fn normalize_clamps_rest_interval() {
        let mut rule: Rule = serde_json::from_value(base_rule_json()).unwrap();
        rule.rest_remind_secs = 1;
        rule.normalize();
        assert_eq!(rule.rest_remind_secs, 300);

        rule.rest_remind_secs = 0;
        rule.normalize();
        assert_eq!(rule.rest_remind_secs, 0, "0 表示关闭，不能被夹成最小值");
    }
}
