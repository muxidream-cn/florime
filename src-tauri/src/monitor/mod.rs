//! 监控主线程：唯一的状态机，也是数据库的唯一写入者。
//!
//! 所有计时都在这里做。前端只接收 `focus://tick` 推送，绝不本地累加。

pub mod category_timeline;
pub mod integrator;
pub mod limits;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Local;
use tauri::{AppHandle, Emitter};

use crate::app_state::AppState;
use crate::config::model::{
    ExceedAction, MatchKind, MonitorMode, Rule, RuleKind, Settings, TriggerMode,
};
use crate::db::ClosedSeg;
use crate::events::{
    AlertPayload, ProcInfo, RestPayload, RuleUsage, Snapshot, EV_ALERT, EV_ALERT_CLOSED,
    EV_REST_CLOSED, EV_STATE, EV_TICK,
};
use crate::win::{close, foreground, hook, idle, kill, procs, protected};
use category_timeline::{CategorySeg, CategoryTimeline, CategoryTotals};

use integrator::{ActiveApp, Integrator};
use limits::{LimitAction, LimitTracker, Phase};

/// 提醒弹窗打开期间的存活复核间隔：略小于 1s 以避开主循环 tick 抖动，实际约 1Hz。
const LIVENESS_PROBE_INTERVAL: Duration = Duration::from_millis(900);

/// 命令投递（前端 / 托盘 → 监控线程）。
#[derive(Debug, Clone)]
pub enum MonitorCmd {
    Pause(bool),
    /// 规则或设置改了，重新读配置
    Reload,
    Snooze {
        rule_id: String,
        secs: u64,
    },
    /// 用户点"立即关闭"
    EnforceNow {
        rule_id: String,
    },
    /// 用户点「强制关闭（结束进程树）」：仅不允许保留后台的清理弹窗提供
    ForceKillTree {
        rule_id: String,
    },
    /// 用户点了提醒窗上的「我知道了」/关闭按钮：只收窗，清理阶段不取消任务
    DismissAlert {
        rule_id: String,
    },
    /// 收起休息提醒并恢复计时（前端倒计时结束 / 用户关闭）
    HideRest,
    Shutdown,
}

enum MonitorMsg {
    Cmd(MonitorCmd),
    Wake,
}

/// 供命令层读取的实时状态。
#[derive(Default)]
pub struct SharedState {
    pub paused: bool,
    pub snapshot: Snapshot,
    pub alerts: HashMap<String, AlertPayload>,
    /// 当前正在展示的休息提醒（独立于超限告警队列）
    pub rest: Option<crate::events::RestPayload>,
}

pub struct MonitorHandle {
    tx: Sender<MonitorMsg>,
    shared: Arc<Mutex<SharedState>>,
    #[allow(dead_code)]
    join: Mutex<Option<JoinHandle<()>>>,
}

impl MonitorHandle {
    pub fn shared(&self) -> Arc<Mutex<SharedState>> {
        self.shared.clone()
    }

    pub fn send(&self, cmd: MonitorCmd) {
        let _ = self.tx.send(MonitorMsg::Cmd(cmd));
    }

    /// 阻塞等待线程收尾，最多 2 秒。
    pub fn shutdown(self) {
        let _ = self.tx.send(MonitorMsg::Cmd(MonitorCmd::Shutdown));
        let join = self.join.lock().expect("monitor join poisoned").take();
        if let Some(handle) = join {
            let (t, r) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = handle.join();
                let _ = t.send(());
            });
            let _ = r.recv_timeout(Duration::from_secs(2));
        }
    }
}

/// 运行期上下文。
struct Ctx {
    app: AppHandle,
    state: Arc<AppState>,
    shared: Arc<Mutex<SharedState>>,
    settings: Settings,
    rules: Vec<Rule>,
    integrator: Integrator,
    category_timeline: CategoryTimeline,
    limits: LimitTracker,
    pending: Vec<ClosedSeg>,
    pending_category: Vec<CategorySeg>,
    last_flush: Instant,
    /// rule_id -> 已用毫秒（不含未闭合段）
    used: HashMap<String, u64>,
    day: chrono::NaiveDate,
    last_fg: Option<foreground::ForegroundInfo>,
    /// running 模式的扫描缓存
    running_active: Vec<ActiveApp>,
    last_scan: Option<Instant>,
    last_emit: Option<Instant>,
    /// 用户点了「立即关闭」的规则 → 告警时刻的 pid（None 表示目标已不可用）
    user_enforce: HashMap<String, Option<u32>>,
    /// 清理阶段最近一轮枚举到的残留进程数（供快照展示）
    cleanup_remaining: HashMap<String, usize>,
    /// 上次存活复核时刻（提醒弹窗打开期间约 1Hz 扫进程）
    last_liveness: Option<Instant>,
    /// rule_id -> 上一 tick 前台窗口是否命中该规则。
    /// 存在即计时模式下后台进程常驻会让 active 恒真，必须靠这个显式边沿识别重开前台。
    fg_hit_prev: HashMap<String, bool>,
    /// rule_id -> 告警发布时间（单调时钟）。存活复核用它保护刚弹出的窗不被立刻收掉。
    alert_published_at: HashMap<String, Instant>,
    /// 休息提醒展示中：冻结计时（与用户手动暂停区分）
    resting: bool,
    /// 休息提醒的最晚结束时刻，防前端挂掉后一直冻结计时
    rest_deadline: Option<Instant>,
    /// 休息结束后等待用户确认的最晚时刻；到点按「继续努力」自动收尾
    rest_confirm_deadline: Option<Instant>,
    /// 规则 -> 当日已提醒到的休息档位（每档 = rest_remind_secs）
    rest_served: HashMap<String, u64>,
}

impl Ctx {
    fn reload_config(&mut self) {
        self.settings = self.state.settings_snapshot();
        self.rules = self.state.rules_snapshot().rules;
        // 老数据可能带有矛盾的开关键组合，进状态机前统一收敛
        for rule in &mut self.rules {
            rule.normalize();
        }
        log::info!(
            "配置已重载: {} 条规则, 触发方式 {:?}",
            self.rules.len(),
            self.settings.trigger_mode
        );
    }

    fn max_gap(&self) -> Duration {
        let interval = match self.settings.trigger_mode {
            TriggerMode::Poll => self.settings.poll_ms,
            TriggerMode::Event => self.settings.heartbeat_ms,
        };
        Duration::from_millis((interval * 3).max(5_000))
    }

    fn tick_interval(&self) -> Duration {
        let ms = match self.settings.trigger_mode {
            TriggerMode::Poll => self.settings.poll_ms,
            TriggerMode::Event => self.settings.heartbeat_ms,
        };
        Duration::from_millis(ms.clamp(200, 10_000))
    }
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) -> MonitorHandle {
    let (tx, rx) = mpsc::channel::<MonitorMsg>();
    let shared = Arc::new(Mutex::new(SharedState::default()));

    let shared_out = shared.clone();
    let state_out = state.clone();
    let tx_out = tx.clone();
    let join = std::thread::Builder::new()
        .name("florime-monitor".into())
        .spawn(move || run(app, state_out, rx, shared_out, tx_out))
        .expect("无法创建监控线程");

    MonitorHandle {
        tx,
        shared,
        join: Mutex::new(Some(join)),
    }
}

fn run(
    app: AppHandle,
    state: Arc<AppState>,
    rx: Receiver<MonitorMsg>,
    shared: Arc<Mutex<SharedState>>,
    tx: Sender<MonitorMsg>,
) {
    let today = Local::now().date_naive();
    let mut ctx = Ctx {
        app,
        state,
        shared,
        settings: Settings::default(),
        rules: Vec::new(),
        integrator: Integrator::new(today),
        category_timeline: CategoryTimeline::new(today),
        limits: LimitTracker::new(),
        pending: Vec::new(),
        pending_category: Vec::new(),
        last_flush: Instant::now(),
        used: HashMap::new(),
        day: today,
        last_fg: None,
        running_active: Vec::new(),
        last_scan: None,
        last_emit: None,
        user_enforce: HashMap::new(),
        cleanup_remaining: HashMap::new(),
        last_liveness: None,
        fg_hit_prev: HashMap::new(),
        alert_published_at: HashMap::new(),
        resting: false,
        rest_deadline: None,
        rest_confirm_deadline: None,
        rest_served: HashMap::new(),
    };
    ctx.settings = ctx.state.settings_snapshot();
    ctx.rules = ctx.state.rules_snapshot().rules;
    for rule in &mut ctx.rules {
        rule.normalize();
    }

    let focus_rule_ids = ctx
        .rules
        .iter()
        .filter(|rule| rule.kind == RuleKind::Focus)
        .map(|rule| rule.id.clone())
        .collect::<Vec<_>>();
    let today_key = today.format("%Y-%m-%d").to_string();
    match crate::db::queries::day_category_stats(
        &ctx.state.db,
        &today_key,
        &today_key,
        &focus_rule_ids,
    ) {
        Ok(stats) => {
            if let Some(stats) = stats.into_iter().find(|stats| stats.day == today_key) {
                ctx.category_timeline.set_baseline(CategoryTotals {
                    total_ms: stats.total_ms.max(0) as u64,
                    focus_ms: stats.focus_ms.max(0) as u64,
                    slack_ms: stats.slack_ms.max(0) as u64,
                    focus_slack_ms: stats.focus_slack_ms.max(0) as u64,
                });
            }
        }
        Err(e) => log::error!("读取今日累计时长失败: {e}"),
    }

    // 初始额度：自然日模式下从 DB 读今天的累计
    ctx.used = load_used(&ctx.state, ctx.day, ctx.settings.limit_reset_mode);

    // 事件模式启动钩子；钩子只负责唤醒，真正采样统一在主循环里做
    let mut _hook_runtime: Option<HookRuntime> = None;
    if ctx.settings.trigger_mode == TriggerMode::Event {
        _hook_runtime = start_hook(tx.clone());
    }

    loop {
        let timeout = ctx.tick_interval();
        let msg = rx.recv_timeout(timeout);

        match msg {
            Ok(MonitorMsg::Cmd(MonitorCmd::Shutdown)) => {
                // 退出前一定要收掉休息提醒并卸载键盘钩子，否则进程残留屏蔽
                finish_rest(&mut ctx);
                crate::win::shield::stop();
                flush(&mut ctx, true);
                let mut out = Vec::new();
                ctx.integrator.close_all(Instant::now(), &mut out);
                if !out.is_empty() {
                    ctx.pending.extend(out);
                    flush(&mut ctx, true);
                }
                close_category_segments(&mut ctx);
                log::info!("监控线程已退出");
                return;
            }
            Ok(MonitorMsg::Cmd(MonitorCmd::Pause(p))) => {
                set_paused(&mut ctx, p);
            }
            Ok(MonitorMsg::Cmd(MonitorCmd::HideRest)) => {
                finish_rest(&mut ctx);
            }
            Ok(MonitorMsg::Cmd(MonitorCmd::Reload)) => {
                // 规则状态已变：用旧分类闭合当前区间，历史已累计区间保持不变
                close_category_segments(&mut ctx);
                let was_event = ctx.settings.trigger_mode == TriggerMode::Event;
                ctx.reload_config();
                // 配置变了，正在展示的休息提醒按旧规则算，直接收起更安全
                finish_rest(&mut ctx);
                let now_event = ctx.settings.trigger_mode == TriggerMode::Event;
                if was_event != now_event {
                    _hook_runtime = None; // Drop 会停掉旧钩子与转发线程
                    if now_event {
                        _hook_runtime = start_hook(tx.clone());
                    }
                }
            }
            Ok(MonitorMsg::Cmd(
                ref cmd @ (MonitorCmd::Snooze { .. }
                | MonitorCmd::EnforceNow { .. }
                | MonitorCmd::ForceKillTree { .. }
                | MonitorCmd::DismissAlert { .. }),
            )) => {
                handle_user_cmd(&mut ctx, cmd.clone());
            }
            Ok(MonitorMsg::Wake) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                flush(&mut ctx, true);
                return;
            }
        }

        step(&mut ctx);
    }
}

/// 钩子线程只负责投递 HWND；转发线程把它变成主循环的 Wake。
///
/// 事件模式的意义是「亚秒级窗口切换归属精度」，不是省 CPU —— 心跳始终保留。
struct HookRuntime {
    _handle: hook::HookHandle,
    _forwarder: std::thread::JoinHandle<()>,
}

fn start_hook(tx_main: Sender<MonitorMsg>) -> Option<HookRuntime> {
    let (tx, rx) = mpsc::channel::<isize>();
    let handle = hook::spawn_foreground_hook(tx);
    let forwarder = std::thread::Builder::new()
        .name("florime-hook-fwd".into())
        .spawn(move || {
            // 钩子线程退出 → Sender 释放 → recv 报错 → 本线程自然结束
            while rx.recv().is_ok() {
                if tx_main.send(MonitorMsg::Wake).is_err() {
                    break;
                }
            }
        })
        .ok()?;
    Some(HookRuntime {
        _handle: handle,
        _forwarder: forwarder,
    })
}

fn set_paused(ctx: &mut Ctx, paused: bool) {
    if ctx.shared.lock().expect("shared poisoned").paused == paused {
        return;
    }
    {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        sh.paused = paused;
    }
    if paused {
        // 暂停＝停止一切提醒：休息提醒会冻结计时，必须同步收起，否则永远停在那儿
        finish_rest(ctx);
        crate::win::shield::stop();
        close_category_segments(ctx);
        let mut out = Vec::new();
        ctx.integrator.close_all(Instant::now(), &mut out);
        if !out.is_empty() {
            absorb(ctx, &out);
            ctx.pending.extend(out);
            flush(ctx, true);
        }

        // 暂停＝停止一切处置：收窗并复位规则状态与清理轮次。
        // 恢复后若仍超额且进程活跃，会重新走一次宽限期提醒（不沿用旧节拍）。
        ctx.limits.reset_all();
        ctx.cleanup_remaining.clear();
        ctx.user_enforce.clear();
        let alert_ids: Vec<String> = ctx
            .shared
            .lock()
            .expect("shared poisoned")
            .alerts
            .keys()
            .cloned()
            .collect();
        for id in alert_ids {
            clear_alert(ctx, &id);
        }
        log::info!("暂停：已重置规则状态与清理轮次");
    }
    log::info!("监控{}", if paused { "已暂停" } else { "已恢复" });
    let _ = ctx.app.emit(EV_STATE, crate::events::paused_payload(paused));

    // 托盘菜单是另一个入口，状态变了要立刻反映，不能等兜底轮询
    let total_ms = ctx
        .shared
        .lock()
        .expect("shared poisoned")
        .snapshot
        .today_total_ms;
    crate::ui::tray::sync_state(&ctx.app, paused, total_ms);
}

fn handle_user_cmd(ctx: &mut Ctx, cmd: MonitorCmd) {
    let now = Instant::now();
    match cmd {
        MonitorCmd::Snooze { rule_id, secs } => {
            ctx.limits.snooze(&rule_id, now, secs);
            ctx.cleanup_remaining.remove(&rule_id);
            let updated = {
                let mut sh = ctx.shared.lock().expect("shared poisoned");
                sh.alerts.get_mut(&rule_id).map(|alert| {
                    alert.phase = "warn".into();
                    alert.remaining = 0;
                    alert.next_check_ts = 0;
                    alert.round = 0;
                    alert.grace_ms = secs * 1000;
                    alert.grace_deadline_ts =
                        chrono::Local::now().timestamp_millis() + (secs as i64) * 1000;
                    alert.clone()
                })
            };
            if let Some(payload) = updated {
                let _ = ctx.app.emit(EV_ALERT, payload);
            }
        }
        MonitorCmd::DismissAlert { rule_id } => {
            // 清理阶段的「我知道了」只是收窗：任务状态保持不变，
            // 下一轮复检会重新弹出提醒，直到命中进程清零。
            if ctx.limits.phase(&rule_id) == Phase::Cleanup {
                log::info!(
                    "规则 {rule_id} 提醒窗已由用户关闭，等待后台清理继续（每轮仍会重弹）"
                );
                crate::ui::windows::hide_toast(&ctx.app);
                return;
            }
            clear_alert(ctx, &rule_id);
        }
        MonitorCmd::EnforceNow { rule_id } => {
            // 清理阶段：用户点「立即关闭」只是"再试一次关闭"。
            // 不能走 force_enforce —— 那会把状态打回 Warning 并重算清理节拍，
            // 提醒窗倒计时会从满格重来。这里保持节拍，失败就按原节奏继续提醒。
            if ctx.limits.phase(&rule_id) == Phase::Cleanup {
                user_close_in_cleanup(ctx, &rule_id);
                return;
            }
            // 记住告警时的 pid：用户点按钮这一刻前台很可能已经切走，
            // 再按规则重新解析会落空（这正是「立即关闭没反应」的第二个原因）。
            let pid = ctx
                .shared
                .lock()
                .expect("shared poisoned")
                .alerts
                .get(&rule_id)
                .map(|a| a.pid);
            ctx.user_enforce.insert(rule_id.clone(), pid);
            ctx.limits.force_enforce(&rule_id, now);
        }
        MonitorCmd::ForceKillTree { rule_id } => {
            force_kill_tree_in_cleanup(ctx, &rule_id);
        }
        _ => {}
    }
}

/// 清理阶段用户点「立即关闭」：做一次显式处置，但**不重置清理节拍**。
///
/// 目标优先级：告警里的进程（用户看到的那个）→ 当前前台命中目标。
/// 处理完仍有残留就保持 `Phase::Cleanup` 与原倒计时，前端超时后显示红字提示；
/// 全部清零则走正常收尾（进冷却 + 收窗 + 审计）。
fn user_close_in_cleanup(ctx: &mut Ctx, rule_id: &str) {
    let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
        ctx.cleanup_remaining.remove(rule_id);
        clear_alert(ctx, rule_id);
        return;
    };

    let alert_pid = ctx
        .shared
        .lock()
        .expect("shared poisoned")
        .alerts
        .get(rule_id)
        .map(|a| a.pid)
        .filter(|pid| *pid != 0);

    let mut targets: Vec<u32> = Vec::new();
    if let Some(pid) = alert_pid {
        let name = procs::find_name(pid).unwrap_or_default();
        let lookup = foreground::resolve_image(pid);
        let has_path = matches!(lookup, foreground::ImageLookup::Full(_));
        let key = lookup.let_key().unwrap_or_else(|| format!("pid:{pid}"));
        if rule.matches(&key, &name, has_path) {
            targets.push(pid);
        }
    }
    if targets.is_empty() {
        if let Some(pid) = foreground_target(ctx, &rule) {
            targets.push(pid);
        }
    }
    if targets.is_empty() {
        ctx.state.db.record_intervention(
            Some(&rule.id),
            &rule.pattern,
            0,
            "user_close",
            "noop",
            None,
        );
        log::info!("清理阶段规则[{}]没有可关闭的目标进程", rule.display_name);
        return;
    }

    // 用户显式指令可越过「仅提醒」策略，但五道闸与身份复核一条都不能少
    let action = if rule.on_exceed == ExceedAction::NotifyOnly {
        ExceedAction::GracefulClose
    } else {
        rule.on_exceed
    };
    let report = dispose_targets(ctx, &rule, targets, action, false, "user_close");
    let result = if report.acted > 0 {
        "ok"
    } else if report.denied > 0 {
        "rejected"
    } else {
        "still_running"
    };
    ctx.state.db.record_intervention(
        Some(&rule.id),
        &rule.pattern,
        alert_pid.unwrap_or(0),
        "user_close",
        result,
        None,
    );

    let guard = ctx.state.guard_snapshot();
    let matched = enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO);
    if matched.is_empty() {
        log::warn!("清理阶段规则[{}]用户关闭成功，按规则已无存活进程", rule.display_name);
        finish_cleanup(ctx, &rule);
        return;
    }

    // 仍有残留：保持清理节拍与提醒窗倒计时不变（不 emit 新告警）
    let remaining = matched.len();
    ctx.cleanup_remaining.insert(rule.id.clone(), remaining);
    log::warn!(
        "清理阶段规则[{}]手动关闭未清干净: 成功 {}, 拒绝 {}, 仍有 {remaining} 个进程（提醒节奏不变）",
        rule.display_name,
        report.acted,
        report.denied
    );
}

/// 清理阶段用户点「强制关闭（结束进程树）」：显式 ForceKill 并展开 root + descendants。
///
/// 这是唯一会展开进程树的入口；身份复核、会话闸、五道闸与展开后二次校验全部保留。
/// 与普通「立即关闭」一样不重置清理节拍，只有命中进程清零才走正常收尾。
fn force_kill_tree_in_cleanup(ctx: &mut Ctx, rule_id: &str) {
    let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
        ctx.cleanup_remaining.remove(rule_id);
        clear_alert(ctx, rule_id);
        return;
    };
    if rule.allow_background || ctx.limits.phase(rule_id) != Phase::Cleanup {
        log::warn!("规则[{rule_id}]不在「不允许后台」的清理阶段，忽略强制树杀请求");
        return;
    }

    let alert_pid = ctx
        .shared
        .lock()
        .expect("shared poisoned")
        .alerts
        .get(rule_id)
        .map(|a| a.pid)
        .filter(|pid| *pid != 0);

    let mut targets: Vec<u32> = Vec::new();
    if let Some(pid) = alert_pid {
        let name = procs::find_name(pid).unwrap_or_default();
        let lookup = foreground::resolve_image(pid);
        let has_path = matches!(lookup, foreground::ImageLookup::Full(_));
        let key = lookup.let_key().unwrap_or_else(|| format!("pid:{pid}"));
        if rule.matches(&key, &name, has_path) {
            targets.push(pid);
        }
    }
    if targets.is_empty() {
        if let Some(pid) = foreground_target(ctx, &rule) {
            targets.push(pid);
        }
    }
    if targets.is_empty() {
        ctx.state.db.record_intervention(
            Some(&rule.id),
            &rule.pattern,
            0,
            "force_kill_tree",
            "noop",
            None,
        );
        log::info!("清理阶段规则[{}]强制树杀没有可关闭的目标进程", rule.display_name);
        return;
    }

    let report = dispose_targets(
        ctx,
        &rule,
        targets,
        ExceedAction::ForceKill,
        true,
        "force_kill_tree",
    );
    let result = if report.acted > 0 {
        "ok"
    } else if report.denied > 0 {
        "rejected"
    } else {
        "still_running"
    };
    ctx.state.db.record_intervention(
        Some(&rule.id),
        &rule.pattern,
        alert_pid.unwrap_or(0),
        "force_kill_tree",
        result,
        None,
    );

    let guard = ctx.state.guard_snapshot();
    let matched = enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO);
    if matched.is_empty() {
        log::warn!("清理阶段规则[{}]强制树杀完成，按规则已无存活进程", rule.display_name);
        finish_cleanup(ctx, &rule);
        return;
    }

    // 仍有残留：保持清理节拍与倒计时不变，只刷新数量（由 EV_TICK 同步给弹窗）
    let remaining = matched.len();
    ctx.cleanup_remaining.insert(rule.id.clone(), remaining);
    {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        if let Some(alert) = sh.alerts.get_mut(rule_id) {
            alert.remaining = remaining;
        }
    }
    log::warn!(
        "清理阶段规则[{}]强制树杀后仍有 {remaining} 个进程（成功 {}，拒绝 {}），保持清理节拍",
        rule.display_name,
        report.acted,
        report.denied
    );
}

fn load_used(state: &AppState, day: chrono::NaiveDate, mode: crate::config::model::LimitResetMode) -> HashMap<String, u64> {
    use crate::config::model::LimitResetMode;
    if mode == LimitResetMode::SinceLaunch {
        return HashMap::new();
    }
    let key = day.format("%Y-%m-%d").to_string();
    match crate::db::queries::used_by_rule(&state.db, &key) {
        Ok(map) => map
            .into_iter()
            .map(|(k, v)| (k, v.max(0) as u64))
            .collect(),
        Err(e) => {
            log::warn!("读取今日额度失败: {e}");
            HashMap::new()
        }
    }
}

fn absorb(ctx: &mut Ctx, segs: &[ClosedSeg]) {
    for seg in segs {
        if let Some(id) = seg.rule_id.as_ref() {
            *ctx.used.entry(id.clone()).or_insert(0) += seg.duration_ms.max(0) as u64;
        }
    }
}

/// 闭合当前分类区间并立即落库；用于暂停、规则切换、休息和退出。
fn close_category_segments(ctx: &mut Ctx) {
    let segs = ctx.category_timeline.close_all_at(Instant::now(), Local::now());
    if !segs.is_empty() {
        ctx.pending_category.extend(segs);
        flush(ctx, true);
    }
}

fn step(ctx: &mut Ctx) {
    let now = Instant::now();
    let wall = Local::now();

    let today = wall.date_naive();
    let paused = ctx.shared.lock().expect("shared poisoned").paused;

    // 休息提醒到达持续时间：默认直接收尾；关闭自动继续时先弹确认，并保持冻结
    if ctx.resting && ctx.rest_deadline.map(|d| now >= d).unwrap_or(false) {
        let payload = ctx
            .shared
            .lock()
            .expect("shared poisoned")
            .rest
            .clone();
        let auto_continue = payload.as_ref().map(|p| p.rest_auto_continue).unwrap_or(true);
        if auto_continue {
            log::info!("休息提醒持续时间结束，收起并恢复计时");
            finish_rest(ctx);
        } else if let Some(p) = payload {
            if ctx.rest_confirm_deadline.is_none() {
                // 用户不回答也不能永久冻结：默认按「继续努力」收尾
                ctx.rest_confirm_deadline = Some(now + Duration::from_secs(300));
                crate::ui::windows::show_rest_confirm(&ctx.app, &p);
                log::info!("休息提醒持续时间结束，等待用户确认是否继续努力");
            }
        } else {
            finish_rest(ctx);
        }
    }
    // 确认等待兜底：超时按继续努力处理
    if ctx.resting
        && ctx
            .rest_confirm_deadline
            .map(|d| now >= d)
            .unwrap_or(false)
    {
        log::info!("休息结束确认超时，按继续努力恢复计时");
        finish_rest(ctx);
    }
    let resting = ctx.resting;
    // 休息期间与手动暂停一样冻结计时与限额，但保留「当前前台」展示
    let frozen = paused || resting;

    // —— 采样 ——
    // 暂停/休息时仍然采样前台，只为概览继续显示当前程序；
    // 但不生成规则命中项、不记录设备使用、不推进限额状态机。
    let mut active: Vec<ActiveApp> = Vec::new();
    let mut current: Option<ProcInfo> = None;
    let mut usage_candidate: Option<ActiveApp> = None;
    // 本 tick 前台窗口命中的规则（不限 mode，供前台重现边沿使用）
    let mut fg_hit_now: HashSet<String> = HashSet::new();

    let idle_exceeded = ctx.settings.idle_pause_enabled
        && idle::idle_duration() >= Duration::from_secs(ctx.settings.idle_pause_secs);

    if !idle_exceeded {
        if let Some(info) = foreground::sample_foreground() {
            ctx.last_fg = Some(info.clone());
            let guard = ctx.state.guard_snapshot();
            let is_self = guard.contains(info.pid);
            let exempt = is_self
                || protected::is_protected(
                    &info.exe_key,
                    &info.exe_name,
                    info.pid,
                    &guard,
                    &ctx.settings.extra_protected,
                )
                || in_list(&info.exe_key, &info.exe_name, &ctx.settings.whitelist)
                || in_list(&info.exe_key, &info.exe_name, &ctx.settings.blacklist);

            let mut tracked = false;
            if !exempt && !frozen {
                for rule in &ctx.rules {
                    if !rule.enabled || rule.paused {
                        continue;
                    }
                    if rule.matches(&info.exe_key, &info.exe_name, info.has_path()) {
                        // 前台命中与 mode 无关：存在即计时模式下后台常驻会让 active 恒真，
                        // 只有这份独立记录才能看到「前台窗口重新出现」的边沿。
                        fg_hit_now.insert(rule.id.clone());
                        if rule.mode == MonitorMode::Foreground {
                            tracked = true;
                            active.push(ActiveApp {
                                rule_id: Some(rule.id.clone()),
                                exe_key: info.exe_key.clone(),
                                exe_name: info.exe_name.clone(),
                                pid: info.pid,
                                mode: rule.mode.as_str().to_string(),
                                title: info.title.clone(),
                                integrity: info.integrity.clone(),
                            });
                        }
                    }
                }
            }

            current = Some(ProcInfo {
                pid: info.pid,
                exe_name: info.exe_name.clone(),
                exe_key: info.exe_key.clone(),
                path: info.path.clone(),
                title: info.title.clone(),
                integrity: info.integrity.clone(),
                tracked,
            });

            // 设备使用轨道：把当前前台按「无规则」记一段，只进统计、不计额度。
            // 白/黑名单说好了不计时，这里一并排除。
            if ctx.settings.track_all_foreground && !exempt && !frozen {
                usage_candidate = Some(ActiveApp {
                    rule_id: None,
                    exe_key: info.exe_key.clone(),
                    exe_name: info.exe_name.clone(),
                    pid: info.pid,
                    mode: "foreground".into(),
                    title: info.title.clone(),
                    integrity: info.integrity.clone(),
                });
            }
        } else {
            ctx.last_fg = None;
        }

        // running 模式：定期扫描进程表（暂停/休息时不扫描、不计规则额度）
        if !frozen {
            let scan_due = ctx
                .last_scan
                .map(|t| now.saturating_duration_since(t) >= Duration::from_millis(ctx.settings.process_scan_ms))
                .unwrap_or(true);
            if scan_due {
                ctx.running_active = scan_running(&ctx.rules, &ctx.settings, &ctx.state.guard_snapshot());
                ctx.last_scan = Some(now);
            }
            active.extend(ctx.running_active.iter().cloned());
        }
    }

    // 同一进程已被规则段覆盖时不重复记使用段，否则统计会翻倍
    if let Some(usage) = usage_candidate {
        if !active.iter().any(|a| a.exe_key == usage.exe_key) {
            active.push(usage);
        }
    }

    // 本轮真正命中进程的规则：分类时间线与限额状态机共用
    let active_rules: HashSet<String> = active
        .iter()
        .filter_map(|a| a.rule_id.clone())
        .collect();

    // —— 计时 ——
    // 跨天切分交给 Integrator：它会按本地 00:00 把正在计时的段切开，
    // 所以这里绝不能先 close_all，否则整段会被错误归到昨天。
    let max_gap = ctx.max_gap();
    let closed = ctx.integrator.step(now, wall, &active, max_gap);
    if !closed.is_empty() {
        absorb(ctx, &closed);
        ctx.pending.extend(closed);
    }

    // 分类时间线只按「是否有专注/摸鱼规则命中」记一段，天然去重。
    let has_focus = ctx
        .rules
        .iter()
        .any(|r| active_rules.contains(&r.id) && r.kind == RuleKind::Focus);
    let has_slack = ctx
        .rules
        .iter()
        .any(|r| active_rules.contains(&r.id) && r.kind == RuleKind::Slack);
    let category_closed = ctx
        .category_timeline
        .step(now, wall, has_focus, has_slack, max_gap);
    if !category_closed.is_empty() {
        ctx.pending_category.extend(category_closed);
    }

    // —— 跨天：切分完成后再重置当日额度 ——
    if today != ctx.day {
        flush(ctx, true);
        ctx.day = today;
        ctx.limits.reset_all();
        ctx.used = load_used(&ctx.state, today, ctx.settings.limit_reset_mode);
        // 休息档位跟着当日额度一起归零：新的一天重新按累计时长计档
        ctx.rest_served.clear();
        log::info!("跨天，额度已按 {:?} 重置", ctx.settings.limit_reset_mode);
    }

    // —— 限额 ——
    let mut used_for_limits: HashMap<String, u64> = ctx.used.clone();
    for rule in &ctx.rules {
        let open = ctx.integrator.open_ms(&rule.id, now);
        if open > 0 {
            *used_for_limits.entry(rule.id.clone()).or_insert(0) += open;
        }
    }
    // 前台窗口重现的边沿：只对「允许保留后台 + 已超限 + 静默阶段」的规则生效。
    // 存在即计时模式下后台常驻使 active 恒真，这里不依赖 active 的下降沿。
    let fg_reappear: HashSet<String> = if frozen {
        HashSet::new()
    } else {
        ctx.rules
            .iter()
            .filter(|rule| {
                if !rule.allow_background {
                    return false;
                }
                let limit_ms = rule.daily_limit_secs.saturating_mul(1000);
                if used_for_limits.get(&rule.id).copied().unwrap_or(0) < limit_ms {
                    return false;
                }
                !ctx.fg_hit_prev.get(&rule.id).copied().unwrap_or(false)
                    && fg_hit_now.contains(&rule.id)
                    && matches!(ctx.limits.phase(&rule.id), Phase::Enforced | Phase::Warning)
            })
            .map(|rule| rule.id.clone())
            .collect()
    };
    if !frozen {
        ctx.fg_hit_prev = fg_hit_now.iter().map(|id| (id.clone(), true)).collect();
    }

    // 额度用满但应用没在跑时不弹窗
    // 暂停时冻结限额状态机：否则清理轮次仍会按节拍重弹提醒窗
    let cleanup_interval = Duration::from_secs(ctx.settings.cleanup_remind_secs.max(5));
    let actions = if frozen {
        Vec::new()
    } else {
        ctx.limits.tick(
            now,
            &used_for_limits,
            &ctx.rules,
            cleanup_interval,
            &active_rules,
            &fg_reappear,
        )
    };
    for action in actions {
        match action {
            LimitAction::Warn(rule_id) => on_warn(ctx, &rule_id, &used_for_limits),
            LimitAction::Enforce(rule_id) => on_enforce(ctx, &rule_id, &used_for_limits),
            LimitAction::CleanupCheck(rule_id) => {
                on_cleanup_check(ctx, &rule_id, &used_for_limits)
            }
        }
    }
    // 弹窗打开期间约 1Hz 复核进程存活：清零立即收窗，残留数变化实时同步到弹窗。
    // 不重发 EV_ALERT（会重置倒计时），重弹节奏仍由 cleanup_remind_secs 控制。
    // 暂停/休息时限额状态机冻结，这里也一并跳过，避免改动被冻结的轮次。
    let liveness_due = ctx
        .last_liveness
        .map(|t| now.saturating_duration_since(t) >= LIVENESS_PROBE_INTERVAL)
        .unwrap_or(true);
    if !frozen && liveness_due {
        ctx.last_liveness = Some(now);
        probe_alert_liveness(ctx, &active_rules);
    }
    // 专注模式的休息提醒：按当日累计档位触发，触发后冻结计时
    if !frozen {
        check_rest_reminders(ctx, &used_for_limits);
    }
    prune_stale_alerts(ctx);
    prune_stale_rest(ctx);

    // —— 落库节流 ——
    flush(ctx, false);

    // —— 推送快照（1Hz）——
    let emit_due = ctx
        .last_emit
        .map(|t| now.saturating_duration_since(t) >= Duration::from_millis(900))
        .unwrap_or(true);
    if emit_due {
        ctx.last_emit = Some(now);
        let snapshot = build_snapshot(ctx, current, &used_for_limits, &active_rules, paused, now);
        let (tray_paused, tray_total) = (snapshot.paused, snapshot.today_total_ms);
        {
            let mut sh = ctx.shared.lock().expect("shared poisoned");
            sh.snapshot = snapshot.clone();
        }
        let _ = ctx.app.emit(EV_TICK, snapshot);
        // 托盘与推送同频，避免「主窗已暂停、托盘还显示暂停监控」的 20 秒滞后
        crate::ui::tray::sync_state(&ctx.app, tray_paused, tray_total);
    }
}

fn build_snapshot(
    ctx: &Ctx,
    current: Option<ProcInfo>,
    used: &HashMap<String, u64>,
    active_rule_ids: &HashSet<String>,
    paused: bool,
    now: Instant,
) -> Snapshot {
    let mut rules = Vec::with_capacity(ctx.rules.len());
    for rule in &ctx.rules {
        let used_ms = used.get(&rule.id).copied().unwrap_or(0);
        let limit_ms = rule.daily_limit_secs.saturating_mul(1000);
        let phase = ctx.limits.phase(&rule.id);
        let state = match phase {
            Phase::Warning => "warning",
            Phase::Enforced => "enforced",
            Phase::Cleanup => "cleanup",
            // 额度已满但进程没在跑：不弹窗，但卡片仍应显示「已超限」
            Phase::Normal if used_ms >= limit_ms => "warning",
            Phase::Normal => "normal",
        }
        .to_string();
        let (pending_procs, next_check_ts) = if phase == Phase::Cleanup {
            (
                ctx.cleanup_remaining.get(&rule.id).copied().unwrap_or(0) as u32,
                ctx.limits
                    .cleanup_next_in(&rule.id)
                    .map(|d| Local::now().timestamp_millis() + d.as_millis() as i64)
                    .unwrap_or(0),
            )
        } else {
            (0, 0)
        };
        let (grace_deadline_ts, grace_ms) = if phase == Phase::Warning {
            ctx.limits
                .grace(&rule.id)
                .map(|(remaining, total)| {
                    (
                        Local::now().timestamp_millis() + remaining.as_millis() as i64,
                        total.as_millis() as u64,
                    )
                })
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        rules.push(RuleUsage {
            rule_id: rule.id.clone(),
            display_name: rule.display_name.clone(),
            used_ms,
            limit_ms,
            remaining_ms: limit_ms.saturating_sub(used_ms),
            state,
            pending_procs,
            next_check_ts,
            grace_deadline_ts,
            grace_ms,
        });
    }
    let totals = ctx.category_timeline.totals();
    let mut focus_active = false;
    let mut slack_active = false;
    for rule in &ctx.rules {
        if !active_rule_ids.contains(&rule.id) {
            continue;
        }
        match rule.kind {
            RuleKind::Focus => focus_active = true,
            RuleKind::Slack => slack_active = true,
        }
    }
    let _ = now;
    Snapshot {
        paused,
        resting: ctx.resting,
        today_total_ms: totals.total_ms,
        today_focus_ms: totals.focus_ms,
        today_slack_ms: totals.slack_ms,
        today_focus_slack_ms: totals.focus_slack_ms,
        current,
        rules,
        focus_active,
        slack_active,
        trigger_mode: match ctx.settings.trigger_mode {
            TriggerMode::Poll => "poll".into(),
            TriggerMode::Event => "event".into(),
        },
        elevated: crate::win::is_elevated(),
    }
}

/// 告警到提醒窗的展示参数（`AlertPayload` 里非业务事实的那部分）。
struct AlertMeta {
    /// warn | cleanup
    phase: &'static str,
    /// 清理阶段按规则仍命中的进程数
    remaining: usize,
    /// 清理阶段下一轮复检的墙钟时间（毫秒）
    next_check_ts: i64,
    /// 清理轮次
    round: u32,
    /// 当前阶段的总时长（宽限期 或 清理提醒间隔）
    window_ms: u64,
    /// 当前阶段的结束时刻（墙钟，毫秒）
    deadline_ts: i64,
}

fn build_alert_payload(
    rule: &Rule,
    pid: u32,
    exe_name: String,
    exe_key: String,
    used_ms: u64,
    meta: AlertMeta,
) -> AlertPayload {
    let limit_ms = rule.daily_limit_secs.saturating_mul(1000);
    AlertPayload {
        alert_id: rule.id.clone(),
        rule_id: rule.id.clone(),
        display_name: rule.display_name.clone(),
        exe_name,
        exe_key,
        pid,
        used_ms,
        limit_ms,
        grace_ms: meta.window_ms,
        grace_deadline_ts: meta.deadline_ts,
        action: match rule.on_exceed {
            ExceedAction::NotifyOnly => "notify_only",
            ExceedAction::GracefulClose => "graceful_close",
            ExceedAction::ForceKill => "force_kill",
        }
        .into(),
        exceeded: used_ms >= limit_ms,
        phase: meta.phase.into(),
        remaining: meta.remaining,
        next_check_ts: meta.next_check_ts,
        round: meta.round,
        allow_background: rule.allow_background,
        message: rule.exceed.message.clone(),
        fullscreen: rule.exceed.fullscreen,
        block_close: rule.exceed.block_close,
        block_shortcuts: rule.exceed.block_shortcuts,
        auto_close_secs: rule.exceed.auto_close_secs,
    }
}

fn on_warn(ctx: &mut Ctx, rule_id: &str, used: &HashMap<String, u64>) {
    let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
        return;
    };
    let used_ms = used.get(rule_id).copied().unwrap_or(0);
    let fg = ctx.last_fg.clone();
    let pid = fg.as_ref().map(|f| f.pid).unwrap_or(0);
    let exe_name = fg
        .as_ref()
        .map(|f| f.exe_name.clone())
        .unwrap_or_else(|| rule.display_name.clone());
    let exe_key = fg
        .as_ref()
        .map(|f| f.exe_key.clone())
        .unwrap_or_default();

    let grace_ms = rule.grace_secs * 1000;
    let payload = build_alert_payload(
        &rule,
        pid,
        exe_name,
        exe_key,
        used_ms,
        AlertMeta {
            phase: "warn",
            remaining: 0,
            next_check_ts: 0,
            round: 0,
            window_ms: grace_ms,
            deadline_ts: Local::now().timestamp_millis() + grace_ms as i64,
        },
    );

    publish_alert(ctx, &payload);
    ctx.state.db.record_intervention(
        Some(&rule.id),
        &payload.exe_key,
        payload.pid,
        "warn",
        "ok",
        None,
    );
    log::warn!(
        "规则[{}]超限: {}/{} ms",
        rule.display_name,
        used_ms,
        payload.limit_ms
    );
}

fn on_enforce(ctx: &mut Ctx, rule_id: &str, used: &HashMap<String, u64>) {
    let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
        return;
    };
    let used_ms = used.get(rule_id).copied().unwrap_or(0);

    // 用户点「立即关闭」：这是显式指令，可以越过「仅提醒」策略，
    // 但五道闸与身份复核一条都不能少。
    let user_entry = ctx.user_enforce.remove(rule_id);
    let user_initiated = user_entry.is_some();
    let user_pid = user_entry.flatten();

    let action = if rule.on_exceed == ExceedAction::NotifyOnly {
        if !user_initiated {
            log::info!("规则[{}]仅提醒，跳过处置", rule.display_name);
            return;
        }
        log::info!("规则[{}]由用户手动触发关闭，跳过「仅提醒」策略", rule.display_name);
        ExceedAction::GracefulClose
    } else {
        rule.on_exceed
    };

    // 用户触发时优先关掉告警里的那个 pid，而不是按当前前台重新解析
    let mut targets: Vec<u32> = Vec::new();
    if let Some(pid) = user_pid {
        let name = procs::find_name(pid).unwrap_or_default();
        let lookup = foreground::resolve_image(pid);
        let has_path = matches!(lookup, foreground::ImageLookup::Full(_));
        let key = lookup.let_key().unwrap_or_else(|| format!("pid:{pid}"));
        if rule.matches(&key, &name, has_path) {
            targets.push(pid);
        } else {
            log::warn!("告警目标 {name}({pid}) 已退出或身份不符，改按规则解析目标");
        }
    }
    if targets.is_empty() {
        targets = resolve_targets(
            &rule,
            ctx.last_fg.as_ref(),
            &ctx.settings,
            &ctx.state.guard_snapshot(),
        );
    }
    if targets.is_empty() {
        // UWP 框宿主反查失败：记审计，不退化去杀框宿主
        let uwp_unresolved = rule.mode == MonitorMode::Foreground
            && ctx
                .last_fg
                .as_ref()
                .map(|f| f.exe_name.eq_ignore_ascii_case("applicationframehost.exe"))
                .unwrap_or(false);
        if uwp_unresolved {
            let fg = ctx.last_fg.as_ref().expect("uwp 分支必然有前台信息");
            ctx.state.db.record_intervention(
                Some(&rule.id),
                &fg.exe_key,
                fg.pid,
                "guard",
                "uwp_unresolved",
                None,
            );
            log::warn!("规则[{}]的 UWP 目标无法反查真实进程，放弃处置", rule.display_name);
        } else {
            log::info!("规则[{}]没有可处置的目标进程", rule.display_name);
        }
        // 没有可关闭的前台目标，但按规则匹配的进程可能还在（后台残留）：
        // 交给收尾逻辑判定是「已清理」还是「等待后台清理」。
        settle_after_enforce(ctx, &rule, used_ms, 0);
        return;
    }

    let audit = if user_initiated { "user_close" } else { "graceful_close" };
    let report = dispose_targets(ctx, &rule, targets, action, false, audit);

    if user_initiated && report.acted == 0 {
        log::warn!(
            "用户请求关闭规则[{}]未生效: 拒绝 {}（受保护 / 无权限 / 未响应 / 已自行退出）",
            rule.display_name,
            report.denied
        );
    }

    log::warn!(
        "规则[{}]处置完成: 成功 {}, 拒绝 {}, 已用 {} ms",
        rule.display_name,
        report.acted,
        report.denied,
        used_ms
    );

    settle_after_enforce(ctx, &rule, used_ms, report.acted);
}

/// 一次处置的结果统计。
struct DisposalReport {
    acted: usize,
    denied: usize,
}

/// 对一批目标 pid 执行处置：身份复核 → 会话闸 → 五道闸 → 关闭/强杀。
///
/// `audit` 用于区分「策略自动关」「用户点按钮关」「清理轮次再关」。
fn dispose_targets(
    ctx: &mut Ctx,
    rule: &Rule,
    targets: Vec<u32>,
    action: ExceedAction,
    // 是否展开进程树（仅用户显式强制关闭时为 true）；自动处置保持只处理同名根进程
    expand_tree: bool,
    audit: &str,
) -> DisposalReport {
    let guard = ctx.state.guard_snapshot();
    let mut acted = 0usize;
    let mut denied = 0usize;

    for pid in targets {
        let exe_name = procs::find_name(pid).unwrap_or_default();
        let lookup = foreground::resolve_image(pid);
        let has_path = matches!(lookup, foreground::ImageLookup::Full(_));
        let exe_key = lookup.let_key().unwrap_or_else(|| format!("pid:{pid}"));

        // 复核身份：解析与处置之间 PID 可能被复用，名字/路径不匹配就放弃
        if !rule.matches(&exe_key, &exe_name, has_path) {
            log::warn!("处置目标 {exe_name}({pid}) 已不匹配规则，跳过");
            continue;
        }

        // 会话闸：读不到会话信息时销毁性动作一律拒绝
        if protected::session_allows(pid) != Some(true) {
            denied += 1;
            ctx.state.db.record_intervention(
                Some(&rule.id),
                &exe_key,
                pid,
                "guard",
                "session",
                None,
            );
            log::warn!("拒绝处置其他会话的进程 {exe_name}({pid})");
            continue;
        }

        // 五道闸
        if protected::is_protected(&exe_key, &exe_name, pid, &guard, &ctx.settings.extra_protected) {
            log::warn!("拒绝处置受保护进程 {exe_name}({pid})");
            ctx.state.db.record_intervention(
                Some(&rule.id),
                &exe_key,
                pid,
                "guard",
                "protected",
                None,
            );
            continue;
        }

        match action {
            ExceedAction::GracefulClose => {
                let outcome = close::request_graceful_close(pid, Duration::from_secs(5));
                match outcome {
                    close::CloseOutcome::Exited => acted += 1,
                    close::CloseOutcome::NotFound => acted += 1,
                    close::CloseOutcome::AccessDenied => {
                        denied += 1;
                        ctx.state.db.record_intervention(
                            Some(&rule.id),
                            &exe_key,
                            pid,
                            audit,
                            "access_denied",
                            None,
                        );
                    }
                    close::CloseOutcome::StillRunning => {
                        ctx.state.db.record_intervention(
                            Some(&rule.id),
                            &exe_key,
                            pid,
                            audit,
                            "still_running",
                            None,
                        );
                        log::info!("{exe_name}({pid}) 未响应 WM_CLOSE");
                    }
                }
            }
            ExceedAction::ForceKill => {
                let mut list = if expand_tree {
                    // 先根后子
                    let root = procs::root_of_same_name(pid);
                    let mut v = vec![root];
                    v.extend(procs::descendants_of(root));
                    v
                } else {
                    vec![procs::root_of_same_name(pid)]
                };
                list.dedup();
                for target in list {
                    // 展开后逐个重新校验
                    let name = procs::find_name(target).unwrap_or_default();
                    let key = foreground::resolve_image(target)
                        .let_key()
                        .unwrap_or_else(|| format!("pid:{target}"));
                    if protected::is_protected(
                        &key,
                        &name,
                        target,
                        &guard,
                        &ctx.settings.extra_protected,
                    ) {
                        continue;
                    }
                    match kill::terminate(target) {
                        kill::KillOutcome::Killed | kill::KillOutcome::NotFound => acted += 1,
                        kill::KillOutcome::AccessDenied => {
                            denied += 1;
                            ctx.state.db.record_intervention(
                                Some(&rule.id),
                                &key,
                                target,
                                "terminate",
                                "access_denied",
                                None,
                            );
                        }
                        kill::KillOutcome::Failed(code) => {
                            denied += 1;
                            ctx.state.db.record_intervention(
                                Some(&rule.id),
                                &key,
                                target,
                                "terminate",
                                "failed",
                                Some(i64::from(code)),
                            );
                        }
                    }
                }
            }
            ExceedAction::NotifyOnly => {}
        }
    }

    DisposalReport { acted, denied }
}

/// 处置后复核残留的间隔与次数。
///
/// 强杀是异步的：`TerminateProcess` 返回时进程可能还挂在进程表里，
/// 立刻扫描会把「正在终止」当成「仍在运行」。给内核一点回收时间再确认。
const SETTLE_RESCAN_DELAY_MS: u64 = 150;
const SETTLE_RESCAN_ATTEMPTS: u32 = 2;

/// 处置之后的收尾判定：按规则匹配的进程清零就收窗进冷却，否则转入「等待后台清理」。
fn settle_after_enforce(ctx: &mut Ctx, rule: &Rule, used_ms: u64, acted: usize) {
    let guard = ctx.state.guard_snapshot();
    let mut matched = enumerate_matches(rule, &ctx.settings, &guard, Duration::ZERO);

    // 只有本轮真的杀过东西才需要复核；没有可处置目标时等待毫无意义
    if !matched.is_empty() && acted > 0 {
        for _ in 0..SETTLE_RESCAN_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(SETTLE_RESCAN_DELAY_MS));
            matched = enumerate_matches(rule, &ctx.settings, &guard, Duration::ZERO);
            if matched.is_empty() {
                log::info!("规则[{}]处置后复核已清零，直接收窗", rule.display_name);
                break;
            }
        }
        if !matched.is_empty() {
            log::info!(
                "规则[{}]处置后复核仍有 {} 个进程存活，进入等待清理",
                rule.display_name,
                matched.len()
            );
        }
    }

    // 节拍从复核结束时刻起算，避免把等待时间也算进清理周期
    let now = Instant::now();

    if matched.is_empty() {
        ctx.cleanup_remaining.remove(&rule.id);
        clear_alert(ctx, &rule.id);
        if acted > 0 {
            log::warn!("规则[{}]处置完成，按规则已无存活进程", rule.display_name);
        }
        return;
    }

    // 用户允许保留后台：残留不再弹窗，按本轮结束收尾（Enforced + 冷却 + 收窗）。
    // 落到 Enforced 后，前台重开的上升沿（limits.rs）会重新进入 Warning 并弹窗，不会漏检。
    if rule.allow_background {
        ctx.cleanup_remaining.remove(&rule.id);
        clear_alert(ctx, &rule.id);
        ctx.limits.finish_cleanup(&rule.id, now);
        log::warn!(
            "规则[{}]处置后仍有 {} 个后台进程，用户允许保留，静默收尾",
            rule.display_name,
            matched.len()
        );
        return;
    }

    // 仍有命中进程（典型是前台窗关了、后台残留还在）：进入等待清理，只提醒不处置后台
    let interval = Duration::from_secs(ctx.settings.cleanup_remind_secs.max(5));
    ctx.limits.enter_cleanup(&rule.id, now, interval);
    let remaining = matched.len();
    ctx.cleanup_remaining.insert(rule.id.clone(), remaining);

    let interval_ms = interval.as_millis() as u64;
    let deadline = Local::now().timestamp_millis() + interval_ms as i64;
    let first = &matched[0];
    let payload = build_alert_payload(
        rule,
        first.pid,
        first.exe_name.clone(),
        first.exe_key.clone(),
        used_ms,
        AlertMeta {
            phase: "cleanup",
            remaining,
            next_check_ts: deadline,
            round: 0,
            window_ms: interval_ms,
            deadline_ts: deadline,
        },
    );
    publish_alert(ctx, &payload);
    log::warn!(
        "规则[{}]处置后仍有 {} 个进程存活，进入等待后台清理",
        rule.display_name,
        remaining
    );
}

/// 清理轮次：复检残留 → 前台仍在则再关一次 → 强制重弹提醒。
fn on_cleanup_check(ctx: &mut Ctx, rule_id: &str, used: &HashMap<String, u64>) {
    let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
        // 规则已被删除：兜底收尾，避免提醒窗常驻
        ctx.cleanup_remaining.remove(rule_id);
        clear_alert(ctx, rule_id);
        return;
    };
    let used_ms = used.get(rule_id).copied().unwrap_or(0);
    let guard = ctx.state.guard_snapshot();
    let mut matched = enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO);

    if matched.is_empty() {
        finish_cleanup(ctx, &rule);
        return;
    }

    // 只有「前台仍是命中目标」才再次关闭；后台残留一律只提醒
    let paused = ctx.shared.lock().expect("shared poisoned").paused;
    if !paused {
        if let Some(pid) = foreground_target(ctx, &rule) {
            if rule.on_exceed == ExceedAction::NotifyOnly {
                log::info!("规则[{}]仅提醒，清理阶段不处置", rule.display_name);
            } else {
                let action = rule.on_exceed;
                let report = dispose_targets(ctx, &rule, vec![pid], action, false, "cleanup_reclose");
                let result = if report.acted > 0 {
                    "ok"
                } else if report.denied > 0 {
                    "rejected"
                } else {
                    "noop"
                };
                ctx.state.db.record_intervention(
                    Some(&rule.id),
                    &rule.pattern,
                    pid,
                    "cleanup_reclose",
                    result,
                    None,
                );
                if report.acted > 0 {
                    matched = enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO);
                    if matched.is_empty() {
                        log::warn!("规则[{}]前台目标已关闭，且无残留进程", rule.display_name);
                        finish_cleanup(ctx, &rule);
                        return;
                    }
                }
                log::info!(
                    "清理轮次再关规则[{}]的前台目标 {pid}: 成功 {}, 拒绝 {}",
                    rule.display_name,
                    report.acted,
                    report.denied
                );
            }
        }
    }

    let remaining = matched.len();
    ctx.cleanup_remaining.insert(rule.id.clone(), remaining);

    let interval_ms = ctx.settings.cleanup_remind_secs.max(5) * 1000;
    let deadline = Local::now().timestamp_millis() + interval_ms as i64;
    let round = ctx.limits.cleanup_round(&rule.id);
    let first = &matched[0];
    let payload = build_alert_payload(
        &rule,
        first.pid,
        first.exe_name.clone(),
        first.exe_key.clone(),
        used_ms,
        AlertMeta {
            phase: "cleanup",
            remaining,
            next_check_ts: deadline,
            round,
            window_ms: interval_ms,
            deadline_ts: deadline,
        },
    );
    publish_alert(ctx, &payload);
    log::warn!(
        "规则[{}]第 {round} 轮清理提醒: 仍有 {remaining} 个进程存活",
        rule.display_name
    );
}

/// 清理完成的统一收尾：进冷却、清告警、记审计。
fn finish_cleanup(ctx: &mut Ctx, rule: &Rule) {
    ctx.limits.finish_cleanup(&rule.id, Instant::now());
    ctx.cleanup_remaining.remove(&rule.id);
    clear_alert(ctx, &rule.id);
    ctx.state.db.record_intervention(
        Some(&rule.id),
        &rule.pattern,
        0,
        "cleanup",
        "done",
        None,
    );
    log::warn!("规则[{}]后台已清理干净，进入冷却", rule.display_name);
}

/// 前台是否仍命中该规则且未被豁免；命中则返回其 pid。
fn foreground_target(ctx: &Ctx, rule: &Rule) -> Option<u32> {
    let fg = ctx.last_fg.as_ref()?;
    if !rule.matches(&fg.exe_key, &fg.exe_name, fg.has_path()) {
        return None;
    }
    let guard = ctx.state.guard_snapshot();
    let exempt = guard.contains(fg.pid)
        || protected::is_protected(
            &fg.exe_key,
            &fg.exe_name,
            fg.pid,
            &guard,
            &ctx.settings.extra_protected,
        )
        || in_list(&fg.exe_key, &fg.exe_name, &ctx.settings.whitelist)
        || in_list(&fg.exe_key, &fg.exe_name, &ctx.settings.blacklist);
    if exempt {
        return None;
    }
    // UWP 框宿主：目标是真实应用进程，反查失败就不处置
    if fg.exe_name.eq_ignore_ascii_case("applicationframehost.exe") {
        return foreground::hosted_pid_of_process(fg.pid);
    }
    Some(fg.pid)
}

/// 把告警写入共享状态并推给提醒窗（含通知与提示音）。
fn publish_alert(ctx: &mut Ctx, payload: &AlertPayload) {
    // 用户关掉了超限提醒：不弹窗、不通知，但宽限期与后续处置照常推进
    let remind = ctx
        .rules
        .iter()
        .any(|r| r.id == payload.rule_id && r.exceed_remind);
    if !remind {
        log::debug!(
            "规则[{}]已关闭超限提醒，跳过提醒窗（处置照常）",
            payload.display_name
        );
        return;
    }
    {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        sh.alerts.insert(payload.rule_id.clone(), payload.clone());
    }
    ctx.alert_published_at
        .insert(payload.rule_id.clone(), Instant::now());
    let _ = ctx.app.emit(EV_ALERT, payload.clone());
    crate::ui::windows::show_toast(&ctx.app, payload);
    if ctx.settings.notify {
        notify_desktop(&ctx.app, payload);
    }
    if ctx.settings.sound {
        crate::win::beep();
    }
    // 全屏 + 不可关闭 + 屏蔽才有意义；看门狗到点与收窗路径都会卸载
    if payload.fullscreen && payload.block_close && payload.block_shortcuts {
        crate::win::shield::start(Duration::from_secs(payload.auto_close_secs));
    }
}

/// 收掉某条规则的告警；最后一个告警消失时才隐藏提醒窗。
fn clear_alert(ctx: &mut Ctx, rule_id: &str) {
    let (existed, empty) = {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        let existed = sh.alerts.remove(rule_id).is_some();
        (existed, sh.alerts.is_empty())
    };
    if !existed {
        return;
    }
    ctx.alert_published_at.remove(rule_id);
    let _ = ctx.app.emit(EV_ALERT_CLOSED, rule_id.to_string());
    if empty {
        crate::ui::windows::hide_toast(&ctx.app);
    }
}

/// 专注模式：按当日累计档位触发休息提醒。
///
/// 档位 = `used_ms / (interval * 1000)`。首次登记直接认账（应用中途启动不补弹历史档位），
/// 档位回落视为跨天归零（只重置，不弹），档位上涨才触发一次。
fn check_rest_reminders(ctx: &mut Ctx, used: &HashMap<String, u64>) {
    let mut trigger: Option<RestPayload> = None;

    for rule in &ctx.rules {
        if !rule.enabled || rule.paused || rule.kind != RuleKind::Focus || rule.rest_remind_secs == 0 {
            continue;
        }
        let interval_ms = rule.rest_remind_secs.saturating_mul(1000);
        if interval_ms == 0 {
            continue;
        }
        let used_ms = used.get(&rule.id).copied().unwrap_or(0);
        let level = used_ms / interval_ms;

        match ctx.rest_served.get(&rule.id).copied() {
            None => {
                ctx.rest_served.insert(rule.id.clone(), level);
            }
            Some(served) if level < served => {
                // 当日额度归零（跨天 / 手动重置）：档位跟着回落，等重新累积到下一档
                ctx.rest_served.insert(rule.id.clone(), level);
            }
            Some(served) if level > served => {
                if trigger.is_none() {
                    // 一次只展示一条；其余规则的超期档位等本轮提醒结束后再补
                    ctx.rest_served.insert(rule.id.clone(), level);
                    let exe_name = ctx
                        .last_fg
                        .as_ref()
                        .map(|f| f.exe_name.clone())
                        .unwrap_or_else(|| rule.pattern.clone());
                    trigger = Some(build_rest_payload(rule, used_ms, exe_name));
                }
            }
            Some(_) => {}
        }
    }

    if let Some(payload) = trigger {
        publish_rest(ctx, payload);
    }
}

fn build_rest_payload(rule: &Rule, used_ms: u64, exe_name: String) -> RestPayload {
    RestPayload {
        alert_id: format!("rest:{}:{}", rule.id, used_ms),
        rule_id: rule.id.clone(),
        display_name: rule.display_name.clone(),
        exe_name,
        message: rule.rest.message.clone(),
        fullscreen: rule.rest.fullscreen,
        block_close: rule.rest.block_close,
        block_shortcuts: rule.rest.block_shortcuts,
        auto_close_secs: rule.rest.auto_close_secs.clamp(5, 300),
        used_ms,
        interval_secs: rule.rest_remind_secs,
        rest_auto_continue: rule.rest_auto_continue,
    }
}

/// 弹出休息提醒并冻结计时。
///
/// 与超限告警完全分开：不写 `shared.alerts`、不进清理轮次、不影响 `prune_stale_alerts`。
fn publish_rest(ctx: &mut Ctx, payload: RestPayload) {
    // 先把当前打开的计时段切掉，提醒期间不再累计（否则一觉醒来直接连跳好几档）
    let mut out = Vec::new();
    ctx.integrator.close_all(Instant::now(), &mut out);
    if !out.is_empty() {
        absorb(ctx, &out);
        ctx.pending.extend(out);
        flush(ctx, true);
    }
    close_category_segments(ctx);

    ctx.resting = true;
    ctx.rest_deadline = Some(Instant::now() + Duration::from_secs(payload.auto_close_secs));
    {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        sh.rest = Some(payload.clone());
    }

    log::info!(
        "规则[{}] 已专注 {} 分钟，触发休息提醒（每 {} 分钟一档）",
        payload.display_name,
        payload.used_ms / 60_000,
        payload.interval_secs / 60
    );

    // 载荷由 show_rest → reveal 在窗口显示前定向推送，避免重复 EV_REST 重置倒计时
    crate::ui::windows::show_rest(&ctx.app, &payload);
    if ctx.settings.notify {
        notify_desktop_rest(&ctx.app, &payload);
    }
    if ctx.settings.sound {
        crate::win::beep();
    }
    // 只有「全屏 + 不可关闭」才屏蔽快捷键；看门狗到点与所有收敛路径都会卸载
    if payload.fullscreen && payload.block_close && payload.block_shortcuts {
        crate::win::shield::start(Duration::from_secs(payload.auto_close_secs));
    }
}

/// 收起休息提醒并恢复计时。幂等：没有提醒在展示时也顺手卸载钩子。
fn finish_rest(ctx: &mut Ctx) {
    let was = ctx.resting;
    let closed = {
        let mut sh = ctx.shared.lock().expect("shared poisoned");
        sh.rest.take()
    };

    ctx.resting = false;
    ctx.rest_deadline = None;
    ctx.rest_confirm_deadline = None;
    crate::win::shield::stop();

    if !was && closed.is_none() {
        return;
    }
    crate::ui::windows::hide_rest(&ctx.app);
    let _ = ctx.app.emit(
        EV_REST_CLOSED,
        closed.map(|p| p.rule_id).unwrap_or_default(),
    );
    log::info!("休息提醒已收起，恢复计时");
}

/// 规则被删除/禁用/改成摸鱼或关掉休息提醒后，正在展示的提醒必须收掉。
fn prune_stale_rest(ctx: &mut Ctx) {
    let stale = {
        let sh = ctx.shared.lock().expect("shared poisoned");
        sh.rest.as_ref().is_some_and(|p| {
            !ctx.rules.iter().any(|r| {
                r.id == p.rule_id
                    && r.enabled
                    && !r.paused
                    && r.kind == RuleKind::Focus
                    && r.rest_remind_secs > 0
            })
        })
    };
    if stale {
        log::info!("休息提醒对应的规则已失效（删除/禁用/改为摸鱼），收起提醒");
        finish_rest(ctx);
    }
}

/// 弹窗打开期间的存活复核（约 1Hz）：只对仍有告警的规则做新鲜进程快照。
///
/// - 目标仍在本 tick 的 `active` 中：只刷新残留数，绝不收窗。
///   进程表快照与前台采样口径不同（FullPath 权限降级时可能误判不匹配），
///   弹窗既然因 active 而弹出，就得以 active 为准，避免刚弹就被收掉。
/// - 刚弹出的告警 2 秒内不参与清零判定，给进程快照留出滞后余量。
/// - 命中进程清零：立即收窗消除迟滞；只有 Cleanup 阶段才额外收尾，
///   Warning 阶段不调 `finish_cleanup`，避免把刚开始的宽限压回 300s 冷却。
/// - 残留数变化：刷新 `cleanup_remaining`，由 1Hz 快照经 `EV_TICK.pending_procs` 同步给弹窗；
///   只改共享载荷、不重发 `EV_ALERT`，避免把倒计时重置为满格。
fn probe_alert_liveness(ctx: &mut Ctx, active_rule_ids: &HashSet<String>) {
    /// 刚弹出的告警在此时间内不做清零判定
    const ALERT_GRACE: Duration = Duration::from_secs(2);
    let now = Instant::now();

    let alerted: Vec<String> = {
        let sh = ctx.shared.lock().expect("shared poisoned");
        sh.alerts.keys().cloned().collect()
    };
    if alerted.is_empty() {
        return;
    }

    let guard = ctx.state.guard_snapshot();
    for rule_id in alerted {
        let Some(rule) = ctx.rules.iter().find(|r| r.id == rule_id).cloned() else {
            continue;
        };
        let phase = ctx.limits.phase(&rule_id);
        if phase != Phase::Warning && phase != Phase::Cleanup {
            continue;
        }

        // 本 tick 前台/扫描仍命中：只刷数量，不收窗
        if active_rule_ids.contains(&rule_id) {
            if phase == Phase::Cleanup {
                let remaining =
                    enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO).len();
                if ctx.cleanup_remaining.get(&rule_id) != Some(&remaining) {
                    ctx.cleanup_remaining.insert(rule_id.clone(), remaining);
                    let mut sh = ctx.shared.lock().expect("shared poisoned");
                    if let Some(alert) = sh.alerts.get_mut(&rule_id) {
                        alert.remaining = remaining;
                    }
                    log::info!("规则[{rule_id}]存活复核更新残留进程数: {remaining}");
                }
            }
            continue;
        }

        // 刚弹出的告警给 2 秒快照滞后余量
        if ctx
            .alert_published_at
            .get(&rule_id)
            .map(|t| now.saturating_duration_since(*t) < ALERT_GRACE)
            .unwrap_or(false)
        {
            continue;
        }

        let matched = enumerate_matches(&rule, &ctx.settings, &guard, Duration::ZERO);
        if matched.is_empty() {
            ctx.cleanup_remaining.remove(&rule_id);
            clear_alert(ctx, &rule_id);
            if phase == Phase::Cleanup {
                ctx.limits.finish_cleanup(&rule_id, now);
            }
            log::info!("规则[{rule_id}]存活复核发现目标已全部退出，立即收窗");
        }
    }
}

fn prune_stale_alerts(ctx: &mut Ctx) {
    let stale: Vec<String> = {
        let sh = ctx.shared.lock().expect("shared poisoned");
        sh.alerts
            .keys()
            .filter(|id| {
                match ctx.rules.iter().find(|r| &r.id == *id) {
                    // 关掉超限提醒后也要收掉正在展示的窗，避免配置已改、窗还挂着
                    Some(rule) if rule.enabled && !rule.paused && rule.exceed_remind => {
                        ctx.limits.phase(id.as_str()) == Phase::Normal
                    }
                    _ => true,
                }
            })
            .cloned()
            .collect()
    };
    for id in stale {
        ctx.cleanup_remaining.remove(&id);
        clear_alert(ctx, &id);
        log::info!("规则 {id} 的告警已失效（删除/禁用或额度复位），已收窗");
    }
}

/// 解析一条规则的处置目标。
fn resolve_targets(
    rule: &Rule,
    fg: Option<&foreground::ForegroundInfo>,
    settings: &Settings,
    guard: &protected::SelfGuard,
) -> Vec<u32> {
    match rule.mode {
        MonitorMode::Foreground => {
            let Some(f) = fg.filter(|f| rule.matches(&f.exe_key, &f.exe_name, f.has_path())) else {
                return Vec::new();
            };
            // UWP / 商店应用：目标是框宿主时先反查真实进程，反查不到就放弃
            if f.exe_name.eq_ignore_ascii_case("applicationframehost.exe") {
                return match foreground::hosted_pid_of_process(f.pid) {
                    Some(real) => vec![real],
                    None => Vec::new(),
                };
            }
            vec![f.pid]
        }
        MonitorMode::Running => scan_running(std::slice::from_ref(rule), settings, guard)
            .into_iter()
            .map(|a| a.pid)
            .collect(),
    }
}

/// 按规则枚举当前全部命中进程（不限 mode，含非前台的后台进程）。
///
/// 与计时/处置同口径：自身进程链、受保护进程、黑白名单一律不计入，
/// 否则这些永远关不掉的进程会让「清理完成」永远无法达成。
///
/// `ttl` 是进程快照的最大缓存时长：清理判定必须传 `Duration::ZERO`，
/// 否则刚关掉的进程会因缓存旧数据而多算一轮残留。
fn enumerate_matches(
    rule: &Rule,
    settings: &Settings,
    guard: &protected::SelfGuard,
    ttl: Duration,
) -> Vec<ActiveApp> {
    let entries = procs::snapshot(ttl);
    let mut out: Vec<ActiveApp> = Vec::new();

    for e in &entries {
        if e.pid == 0 || e.pid == 4 {
            continue;
        }
        let name_lower = e.exe_name.to_lowercase();
        let candidate = match rule.match_kind {
            MatchKind::ExeName => name_lower == rule.pattern,
            MatchKind::FullPath => rule.pattern.ends_with(&name_lower),
        };
        if !candidate {
            continue;
        }

        let lookup = foreground::resolve_image(e.pid);
        let (exe_key, has_path) = match &lookup {
            foreground::ImageLookup::Full(p) => (crate::config::normalize_exe_key(p), true),
            foreground::ImageLookup::NameOnly(n) => (crate::config::normalize_exe_key(n), false),
            foreground::ImageLookup::Denied => (format!("pid:{}", e.pid), false),
        };

        if !rule.matches(&exe_key, &e.exe_name, has_path) {
            continue;
        }
        if guard.contains(e.pid)
            || protected::is_protected(
                &exe_key,
                &e.exe_name,
                e.pid,
                guard,
                &settings.extra_protected,
            )
            || in_list(&exe_key, &e.exe_name, &settings.whitelist)
            || in_list(&exe_key, &e.exe_name, &settings.blacklist)
        {
            continue;
        }

        out.push(ActiveApp {
            rule_id: Some(rule.id.clone()),
            exe_key,
            exe_name: e.exe_name.clone(),
            pid: e.pid,
            mode: rule.mode.as_str().to_string(),
            title: String::new(),
            integrity: match lookup {
                foreground::ImageLookup::Full(_) => "ok",
                foreground::ImageLookup::NameOnly(_) => "unknown",
                foreground::ImageLookup::Denied => "denied",
            }
            .into(),
        });
    }
    out
}

/// 扫描当前有哪些进程命中 `running` 规则。
fn scan_running(rules: &[Rule], settings: &Settings, guard: &protected::SelfGuard) -> Vec<ActiveApp> {
    let mut out: Vec<ActiveApp> = Vec::new();
    for rule in rules
        .iter()
        .filter(|r| r.enabled && !r.paused && r.mode == MonitorMode::Running)
    {
        out.extend(enumerate_matches(
            rule,
            settings,
            guard,
            Duration::from_millis(settings.process_scan_ms.max(1000)),
        ));
    }
    out
}

fn in_list(exe_key: &str, exe_name: &str, list: &[String]) -> bool {
    let name = exe_name.to_lowercase();
    list.iter()
        .any(|p| p.eq_ignore_ascii_case(exe_key) || p.eq_ignore_ascii_case(&name))
}

fn flush(ctx: &mut Ctx, force: bool) {
    if ctx.pending.is_empty() && ctx.pending_category.is_empty() {
        return;
    }
    let due = force
        || ctx.pending.len() >= 20
        || ctx.pending_category.len() >= 20
        || ctx.last_flush.elapsed() >= Duration::from_secs(30);
    if !due {
        return;
    }
    let batch = std::mem::take(&mut ctx.pending);
    if !batch.is_empty() {
        if let Err(e) = ctx.state.db.write_batch(&batch) {
            log::error!("批量落库失败（丢弃 {} 段）: {e}", batch.len());
        }
    }
    let category_batch = std::mem::take(&mut ctx.pending_category);
    if !category_batch.is_empty() {
        if let Err(e) = ctx.state.db.write_category_batch(&category_batch) {
            log::error!("分类区间落库失败（丢弃 {} 段）: {e}", category_batch.len());
        }
    }
    ctx.last_flush = Instant::now();
}

fn notify_desktop(app: &AppHandle, payload: &AlertPayload) {
    use tauri_plugin_notification::NotificationExt;
    let grace_secs = payload.grace_ms / 1000;

    let (title, body) = if payload.phase == "cleanup" {
        (
            format!("{} 等待后台清理", payload.display_name),
            format!(
                "今日已用 {}，上限 {}。后台仍有 {} 个进程，请自行关闭；若前台仍在，{} 秒后会再次尝试关闭。",
                format_minutes(payload.used_ms),
                format_minutes(payload.limit_ms),
                payload.remaining,
                grace_secs
            ),
        )
    } else {
        (
            format!("{} 超时提醒", payload.display_name),
            format!(
                "今日已用 {}，上限 {}。{}",
                format_minutes(payload.used_ms),
                format_minutes(payload.limit_ms),
                match payload.action.as_str() {
                    "graceful_close" => format!("{grace_secs} 秒后自动关闭窗口"),
                    "force_kill" => format!("{grace_secs} 秒后强制结束进程"),
                    _ => "请注意休息".to_string(),
                }
            ),
        )
    };

    let res = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
    if let Err(e) = res {
        log::warn!("系统通知发送失败: {e}");
    }
}

/// 休息提醒的系统通知：文案与窗口保持一致，空文案用默认句子。
fn notify_desktop_rest(app: &AppHandle, payload: &RestPayload) {
    use tauri_plugin_notification::NotificationExt;

    let body = if payload.message.trim().is_empty() {
        format!(
            "你已经专注 {} 分钟，起来活动一下吧。",
            payload.used_ms / 60_000
        )
    } else {
        payload.message.clone()
    };
    let res = app
        .notification()
        .builder()
        .title(format!("{} · 休息提醒", payload.display_name))
        .body(body)
        .show();
    if let Err(e) = res {
        log::warn!("休息提醒通知发送失败: {e}");
    }
}

fn format_minutes(ms: u64) -> String {
    let total_min = ms / 60_000;
    format!("{} 小时 {} 分", total_min / 60, total_min % 60)
}

/// 便于 `ImageLookup` 取 key 的小工具。
trait ImageKey {
    fn let_key(&self) -> Option<String>;
}

impl ImageKey for foreground::ImageLookup {
    fn let_key(&self) -> Option<String> {
        match self {
            foreground::ImageLookup::Full(p) => Some(crate::config::normalize_exe_key(p)),
            foreground::ImageLookup::NameOnly(n) => Some(crate::config::normalize_exe_key(n)),
            foreground::ImageLookup::Denied => None,
        }
    }
}
