//! 每规则独立的限额状态机。
//!
//! Normal ─超限→ Warning(已通知) ─宽限期满→ Enforced(已执行)
//!                                        ├─ 命中进程已清零 → 冷却 5min → Warning（不重复弹窗）
//!                                        └─ 仍有命中进程 → Cleanup(每 N 秒复检 + 重弹提醒)
//!
//! Cleanup 只以「按规则匹配的进程清零」退出，不按时间退出：清完再进冷却，
//! 冷却结束回到 Warning 且不再弹窗。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::config::model::{ExceedAction, Rule};

const COOLDOWN: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Normal,
    Warning,
    Enforced,
    /// 宽限期已过、按规则仍能匹配到进程：只提醒，不再主动处置后台残留
    Cleanup,
}

struct RuleLimit {
    phase: Phase,
    /// 宽限期结束时刻（monotonic）
    grace_deadline: Option<Instant>,
    /// 本轮宽限期的总时长，供首页倒计时显示进度
    grace_total: Duration,
    /// 处置冷却结束时刻
    cooldown_until: Option<Instant>,
    /// 清理阶段下一轮复检时刻（monotonic）
    cleanup_next: Option<Instant>,
    /// 清理阶段已进行到第几轮（0 表示尚未开始）
    cleanup_round: u32,
    /// 本轮超限是否已经弹过窗
    notified: bool,
    /// 上一 tick 是否观察到该规则有存活/前台命中的目标
    ///
    /// 用来识别「进程被关掉后又重新打开」的上升沿：只有这个跃迁才值得重弹提醒，
    /// 进程一直没断过时要保持沉默，否则「仅提醒」策略会变成不停弹窗。
    seen_active: bool,
}

impl RuleLimit {
    fn fresh() -> Self {
        Self {
            phase: Phase::Normal,
            grace_deadline: None,
            grace_total: Duration::ZERO,
            cooldown_until: None,
            cleanup_next: None,
            cleanup_round: 0,
            notified: false,
            seen_active: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LimitAction {
    /// 首次超限，弹窗提醒
    Warn(String),
    /// 宽限期满，执行一次处置
    Enforce(String),
    /// 清理阶段到点：复检残留、必要时再次关闭前台，并强制重弹提醒
    CleanupCheck(String),
}

pub struct LimitTracker {
    states: HashMap<String, RuleLimit>,
}

impl LimitTracker {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// 用户点"再给 N 分钟"。
    ///
    /// 必须同时清掉冷却与清理节拍：否则在 Enforced 冷却期内点延期，下一次 tick 会立刻重新处置；
    /// 在清理期点延期则会被清理轮次反复重弹。
    pub fn snooze(&mut self, rule_id: &str, now: Instant, secs: u64) {
        let st = self
            .states
            .entry(rule_id.to_string())
            .or_insert_with(RuleLimit::fresh);
        st.phase = Phase::Warning;
        st.cooldown_until = None;
        st.cleanup_next = None;
        st.cleanup_round = 0;
        st.grace_deadline = Some(now + Duration::from_secs(secs));
        st.grace_total = Duration::from_secs(secs);
        // 已经通知过，到期只执行不再弹窗
        st.notified = true;
        // 点延期时目标仍在前台：标记为已见过，避免下一 tick 被当成「重新出现」而重弹
        st.seen_active = true;
        log::info!("规则 {rule_id} 延期 {secs}s");
    }

    /// 立即进入处置（用户点"立即关闭"）。
    pub fn force_enforce(&mut self, rule_id: &str, now: Instant) {
        let st = self
            .states
            .entry(rule_id.to_string())
            .or_insert_with(RuleLimit::fresh);
        st.phase = Phase::Warning;
        st.cooldown_until = None;
        st.cleanup_next = None;
        st.cleanup_round = 0;
        st.grace_deadline = Some(now);
        st.grace_total = Duration::ZERO;
        st.notified = true;
        // 用户点「立即关闭」时目标在场，避免下一 tick 被当成重新出现而重弹
        st.seen_active = true;
    }

    /// 处置后仍有命中进程：转入清理阶段，按 `interval` 节拍复检。
    pub fn enter_cleanup(&mut self, rule_id: &str, now: Instant, interval: Duration) {
        let st = self
            .states
            .entry(rule_id.to_string())
            .or_insert_with(RuleLimit::fresh);
        st.phase = Phase::Cleanup;
        st.grace_deadline = None;
        st.grace_total = Duration::ZERO;
        st.cooldown_until = None;
        st.cleanup_next = Some(now + interval);
        st.cleanup_round = 0;
        st.notified = true;
        log::info!(
            "规则 {rule_id} 进入等待后台清理，每 {}s 复检",
            interval.as_secs()
        );
    }

    /// 命中进程已清零：收尾并进入常规冷却，避免清理完立刻又弹一轮。
    pub fn finish_cleanup(&mut self, rule_id: &str, now: Instant) {
        let st = self
            .states
            .entry(rule_id.to_string())
            .or_insert_with(RuleLimit::fresh);
        st.phase = Phase::Enforced;
        st.grace_deadline = None;
        st.grace_total = Duration::ZERO;
        st.cleanup_next = None;
        st.cleanup_round = 0;
        st.cooldown_until = Some(now + COOLDOWN);
    }

    pub fn phase(&self, rule_id: &str) -> Phase {
        self.states
            .get(rule_id)
            .map(|s| s.phase)
            .unwrap_or(Phase::Normal)
    }

    /// 当前清理轮次（非清理阶段为 0）。
    pub fn cleanup_round(&self, rule_id: &str) -> u32 {
        self.states
            .get(rule_id)
            .filter(|s| s.phase == Phase::Cleanup)
            .map(|s| s.cleanup_round)
            .unwrap_or(0)
    }

    /// 距离下一轮清理复检还有多久（非清理阶段为 None）。
    pub fn cleanup_next_in(&self, rule_id: &str) -> Option<Duration> {
        let st = self.states.get(rule_id).filter(|s| s.phase == Phase::Cleanup)?;
        st.cleanup_next
            .map(|t| t.saturating_duration_since(Instant::now()))
    }

    /// 当前宽限期的剩余时间与总时长（非宽限阶段为 None）。
    pub fn grace(&self, rule_id: &str) -> Option<(Duration, Duration)> {
        let st = self.states.get(rule_id).filter(|s| s.phase == Phase::Warning)?;
        let deadline = st.grace_deadline?;
        Some((
            deadline.saturating_duration_since(Instant::now()),
            st.grace_total,
        ))
    }

    pub fn reset_all(&mut self) {
        self.states.clear();
    }

    /// 推进一步。`used` 是 rule_id -> 已用毫秒（含当前未闭合段）。
    ///
    /// `cleanup_interval` 来自设置项「清理失败提醒间隔」，每轮进入时读取，
    /// 改设置后无需重启即可生效。
    ///
    /// `active` 是本轮真正命中进程的规则集合：额度已用满但应用没在跑时
    /// （典型是启动后读到今天的历史用量）不弹窗，等它出现再提醒。
    ///
    /// `fg_reappear` 是本轮「前台窗口重新出现」的规则集合，由监控线程单独识别。
    /// 存在即计时模式下后台进程常驻会让 `active` 恒真，看不到下降沿，
    /// 因此必须靠这个显式边沿才能让重开前台重新进入提醒流程。
    pub fn tick(
        &mut self,
        now: Instant,
        used: &HashMap<String, u64>,
        rules: &[Rule],
        cleanup_interval: Duration,
        active: &HashSet<String>,
        fg_reappear: &HashSet<String>,
    ) -> Vec<LimitAction> {
        // 清理已被删除的规则的残留状态
        self.states.retain(|id, _| rules.iter().any(|r| &r.id == id));

        let mut out = Vec::new();
        for rule in rules {
            if rule.daily_limit_secs == 0 {
                continue;
            }
            let limit_ms = rule.daily_limit_secs.saturating_mul(1000);
            let used_ms = used.get(&rule.id).copied().unwrap_or(0);
            let st = self
                .states
                .entry(rule.id.clone())
                .or_insert_with(RuleLimit::fresh);

            if used_ms < limit_ms {
                // 用量回落到阈值以下（跨天重置 / 手动改大限额）→ 复位
                *st = RuleLimit::fresh();
                continue;
            }

            if !rule.enabled || rule.paused {
                // 规则被禁用：状态复位，残留告警由监控线程的兜底清理收窗
                if st.phase != Phase::Normal {
                    *st = RuleLimit::fresh();
                }
                continue;
            }

            // 前台窗口重现：存在即计时模式下后台常驻使 active 恒真，
            // 这里不依赖 active 的上升沿，直接重置成新一轮超限并弹窗。
            // mod.rs 已经按 allow_background 过滤，这里再守一道，
            // 避免未来调用方误传导致普通规则被重弹。
            if rule.allow_background && fg_reappear.contains(&rule.id) {
                log::info!("规则[{}]前台目标重新出现，重置为新一轮超限", rule.id);
                *st = RuleLimit::fresh();
                st.phase = Phase::Warning;
                st.grace_deadline = Some(now + Duration::from_secs(rule.grace_secs));
                st.grace_total = Duration::from_secs(rule.grace_secs);
                st.notified = true;
                // 本 tick 已确认前台在场，别再把自己判成新的上升沿
                st.seen_active = true;
                out.push(LimitAction::Warn(rule.id.clone()));
                continue;
            }

            // 目标在场的观测：用于识别「被关掉后又重新打开」的上升沿
            let is_active = active.contains(&rule.id);
            let was_active = st.seen_active;
            st.seen_active = is_active;

            // 目标重新出现：静默不该继续，重置成新一轮超限，
            // 下面 Normal 分支会立刻重弹提醒并重新开始宽限期。
            // 冷却到期后 phase 已静默转成 Warning，也必须在这里一并覆盖，
            // 否则会出现「计时在走，但既没提醒也不再关闭」的空窗。
            if is_active
                && !was_active
                && matches!(st.phase, Phase::Enforced | Phase::Warning)
            {
                log::info!("规则[{}]目标重新出现，重置为新一轮超限", rule.id);
                *st = RuleLimit::fresh();
                // 本 tick 已经确认它在场，别再把自己判成新的上升沿
                st.seen_active = true;
            }

            match st.phase {
                Phase::Normal => {
                    // 进程没在跑就不打扰：等目标真正出现（前台命中 / 存在即计时扫描到）
                    // 再开始宽限期，避免启动时按历史用量误弹超时提醒。
                    if !is_active {
                        continue;
                    }
                    st.phase = Phase::Warning;
                    st.grace_deadline = Some(now + Duration::from_secs(rule.grace_secs));
                    st.grace_total = Duration::from_secs(rule.grace_secs);
                    st.notified = false;
                    out.push(LimitAction::Warn(rule.id.clone()));
                    st.notified = true;
                }
                Phase::Warning => {
                    if let Some(deadline) = st.grace_deadline {
                        if now >= deadline {
                            st.phase = Phase::Enforced;
                            st.grace_total = Duration::ZERO;
                            st.cooldown_until = Some(
                                now + if rule.on_exceed == ExceedAction::NotifyOnly {
                                    // 仅提醒模式拉长冷却，避免每 60 秒弹一次
                                    Duration::from_secs(600)
                                } else {
                                    COOLDOWN
                                },
                            );
                            out.push(LimitAction::Enforce(rule.id.clone()));
                        }
                    }
                }
                Phase::Enforced => {
                    if let Some(until) = st.cooldown_until {
                        if now >= until {
                            // 冷却结束仍超限 → 重新开始宽限期，但不再弹窗
                            st.phase = Phase::Warning;
                            st.grace_deadline = Some(now + Duration::from_secs(rule.grace_secs));
                            st.grace_total = Duration::from_secs(rule.grace_secs);
                        }
                    }
                }
                Phase::Cleanup => {
                    let due = st.cleanup_next.map(|t| now >= t).unwrap_or(true);
                    if due {
                        st.cleanup_next = Some(now + cleanup_interval);
                        st.cleanup_round = st.cleanup_round.saturating_add(1);
                        out.push(LimitAction::CleanupCheck(rule.id.clone()));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{MatchKind, MonitorMode, NotifyConfig, RuleKind};

    const INTERVAL: Duration = Duration::from_secs(15);

    fn rule() -> Rule {
        Rule {
            id: "r1".into(),
            display_name: "测试".into(),
            match_kind: MatchKind::ExeName,
            pattern: "test.exe".into(),
            mode: MonitorMode::Foreground,
            daily_limit_secs: 60,
            grace_secs: 15,
            on_exceed: ExceedAction::GracefulClose,
            allow_background: false,
            enabled: true,
            paused: false,
            kind: RuleKind::Slack,
            rest_remind_secs: 0,
            rest_auto_continue: true,
            rest: NotifyConfig::default(),
            exceed_remind: true,
            exceed: NotifyConfig::default(),
        }
    }

    fn used_over() -> HashMap<String, u64> {
        HashMap::from([("r1".to_string(), 120_000u64)])
    }

    fn active() -> HashSet<String> {
        HashSet::from(["r1".to_string()])
    }

    #[test]
    fn cleanup_repeats_every_interval_until_finished() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();

        // 超限 → 弹窗提醒，宽限期开始
        let actions = tracker.tick(t0, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Warn(id)] if id == "r1"));
        assert_eq!(tracker.phase("r1"), Phase::Warning);

        // 宽限未到：不处置
        let actions = tracker.tick(t0 + Duration::from_secs(14), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(actions.is_empty());

        // 宽限到期：处置一次
        let actions = tracker.tick(t0 + Duration::from_secs(15), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(id)] if id == "r1"));
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // 仍有残留 → 进入等待清理
        tracker.enter_cleanup("r1", t0 + Duration::from_secs(15), INTERVAL);
        assert_eq!(tracker.phase("r1"), Phase::Cleanup);

        // 节拍未到不复检
        let actions = tracker.tick(t0 + Duration::from_secs(20), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(actions.is_empty(), "节拍未到不应复检");

        // 第一轮
        let actions = tracker.tick(t0 + Duration::from_secs(30), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::CleanupCheck(_)]));
        assert_eq!(tracker.cleanup_round("r1"), 1);

        // 第二轮：又过一个间隔
        let actions = tracker.tick(t0 + Duration::from_secs(44), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(actions.is_empty());
        let actions = tracker.tick(t0 + Duration::from_secs(45), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::CleanupCheck(_)]));
        assert_eq!(tracker.cleanup_round("r1"), 2);

        // 清理完成 → 冷却，冷却结束回 Warning 但不再弹窗
        tracker.finish_cleanup("r1", t0 + Duration::from_secs(45));
        assert_eq!(tracker.phase("r1"), Phase::Enforced);
        assert_eq!(tracker.cleanup_round("r1"), 0);
        assert!(tracker.cleanup_next_in("r1").is_none());

        let actions = tracker.tick(
            t0 + Duration::from_secs(45 + 299),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &HashSet::new(),
        );
        assert!(actions.is_empty());
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        let actions = tracker.tick(
            t0 + Duration::from_secs(45 + 300),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &HashSet::new(),
        );
        assert!(actions.is_empty(), "冷却结束回 Warning 时不应重新弹窗");
        assert_eq!(tracker.phase("r1"), Phase::Warning);
    }

    #[test]
    fn snooze_cancels_cleanup_loop() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();

        tracker.enter_cleanup("r1", t0, INTERVAL);
        tracker.snooze("r1", t0, 300);
        assert_eq!(tracker.phase("r1"), Phase::Warning);

        for s in 0..300u64 {
            let actions = tracker.tick(t0 + Duration::from_secs(s), &used, &rules, INTERVAL, &active(), &HashSet::new());
            assert!(
                actions.is_empty(),
                "延期期间不应再产出清理轮次或处置（s={s}）"
            );
        }

        let actions = tracker.tick(t0 + Duration::from_secs(300), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(_)]));
        assert_eq!(tracker.phase("r1"), Phase::Enforced);
    }

    /// 当天额度早已用满、但目标进程没在跑（重启后读到历史用量）时不得弹窗，
    /// 等进程真正出现再提醒。
    #[test]
    fn no_warn_when_target_not_running() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let nothing: HashSet<String> = HashSet::new();

        for _ in 0..3 {
            let actions = tracker.tick(t0, &used, &rules, INTERVAL, &nothing, &HashSet::new());
            assert!(actions.is_empty(), "进程不存在时不应弹超时提醒");
        }
        assert_eq!(tracker.phase("r1"), Phase::Normal);

        // 进程出现 → 立即提醒并开始宽限期
        let actions = tracker.tick(t0, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Warn(_)]));
        assert_eq!(tracker.phase("r1"), Phase::Warning);
    }

    /// 处置完成进入冷却后，用户把程序关掉再打开：必须重新弹提醒并重新开始宽限期，
    /// 否则会出现「计时在走，但既没提醒也不再关闭」的空窗。
    #[test]
    fn reappear_after_enforce_rewarns() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let nothing: HashSet<String> = HashSet::new();

        // 超限 → 提醒 → 宽限期满 → 处置
        let actions = tracker.tick(t0, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Warn(_)]));
        let t1 = t0 + Duration::from_secs(15);
        let actions = tracker.tick(t1, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(_)]));
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // 程序被关掉：冷却期内保持沉默
        let t2 = t1 + Duration::from_secs(1);
        assert!(tracker.tick(t2, &used, &rules, INTERVAL, &nothing, &HashSet::new()).is_empty());

        // 用户手动重开并切到前台：立刻重新提醒
        let t3 = t2 + Duration::from_secs(1);
        let actions = tracker.tick(t3, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(
            matches!(actions.as_slice(), [LimitAction::Warn(id)] if id == "r1"),
            "目标重新出现时应重新弹提醒"
        );
        assert_eq!(tracker.phase("r1"), Phase::Warning);

        // 新的宽限期从重现时刻重新起算
        assert!(tracker
            .tick(
                t3 + Duration::from_secs(14),
                &used,
                &rules,
                INTERVAL,
                &active(),
                &HashSet::new()
            )
            .is_empty());
        let actions = tracker.tick(
            t3 + Duration::from_secs(15),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &HashSet::new(),
        );
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(_)]));
    }

    /// 允许保留后台时，处置收尾直接 `finish_cleanup`（Enforced + 冷却，不进 Cleanup）。
    /// 用户关掉前台后重开，上升沿必须重新弹窗，否则会出现「计时在走但不提醒」的空窗。
    #[test]
    fn allowed_background_then_reopen_rewarns() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let nothing: HashSet<String> = HashSet::new();

        tracker.tick(t0, &used, &rules, INTERVAL, &active(), &HashSet::new());
        // 对应 settle_after_enforce 中 allow_background 的静默分支
        tracker.finish_cleanup("r1", t0 + Duration::from_secs(1));
        assert_eq!(tracker.phase("r1"), Phase::Enforced);
        assert!(tracker.cleanup_next_in("r1").is_none());

        // 前台已关、后台仍在：冷却期内保持沉默
        let t1 = t0 + Duration::from_secs(2);
        assert!(tracker.tick(t1, &used, &rules, INTERVAL, &nothing, &HashSet::new()).is_empty());

        // 重开前台：立刻重新提醒并重新起宽限
        let t2 = t1 + Duration::from_secs(1);
        let actions = tracker.tick(t2, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(matches!(actions.as_slice(), [LimitAction::Warn(id)] if id == "r1"));
        assert_eq!(tracker.phase("r1"), Phase::Warning);
        assert!(tracker
            .tick(t2 + Duration::from_secs(14), &used, &rules, INTERVAL, &active(), &HashSet::new())
            .is_empty());
        let actions = tracker.tick(
            t2 + Duration::from_secs(15),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &HashSet::new(),
        );
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(_)]));
    }

    /// 目标一直没退出（用户无视提醒，典型是「仅提醒」策略）：冷却结束只静默转 Warning，
    /// 不得因为仍然活跃就重弹，否则会退化成每半分钟弹一次。
    #[test]
    fn staying_active_during_cooldown_does_not_rewarn() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();

        tracker.tick(t0, &used, &rules, INTERVAL, &active(), &HashSet::new());
        tracker.tick(t0 + Duration::from_secs(15), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // 冷却期内持续活跃：不得重弹
        let actions = tracker.tick(t0 + Duration::from_secs(60), &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(actions.is_empty(), "冷却期内持续活跃不得重弹");
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // 冷却结束仍是活跃状态：只能静默转 Warning
        let until = t0 + Duration::from_secs(15) + COOLDOWN;
        let actions = tracker.tick(until, &used, &rules, INTERVAL, &active(), &HashSet::new());
        assert!(actions.is_empty(), "冷却结束仍活跃时不应重新弹窗");
        assert_eq!(tracker.phase("r1"), Phase::Warning);
    }

    /// 原漏洞：冷却 300s 到期后 phase 已静默转成 Warning，
    /// 此时把目标关掉再打开仍必须重新弹窗（上升沿不能只认 Enforced）。
    #[test]
    fn reopen_after_silent_cooldown_transition_rewarns() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let nothing: HashSet<String> = HashSet::new();
        let none = HashSet::new();

        tracker.tick(t0, &used, &rules, INTERVAL, &active(), &none);
        let enforced_at = t0 + Duration::from_secs(15);
        tracker.tick(enforced_at, &used, &rules, INTERVAL, &active(), &none);
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // 关掉目标 → 产生下降沿；一直冷却到静默转成 Warning
        tracker.tick(enforced_at + Duration::from_secs(1), &used, &rules, INTERVAL, &nothing, &none);
        let silent_at = enforced_at + COOLDOWN;
        tracker.tick(silent_at, &used, &rules, INTERVAL, &active(), &none);
        assert_eq!(tracker.phase("r1"), Phase::Warning, "冷却到期应静默转 Warning");

        // 再关掉产生下降沿
        tracker.tick(silent_at + Duration::from_secs(1), &used, &rules, INTERVAL, &nothing, &none);
        // 重新打开：必须重弹
        let reopen_at = silent_at + Duration::from_secs(2);
        let actions = tracker.tick(reopen_at, &used, &rules, INTERVAL, &active(), &none);
        assert!(
            matches!(actions.as_slice(), [LimitAction::Warn(id)] if id == "r1"),
            "冷却到期后重开也应重新弹窗"
        );
        assert_eq!(tracker.phase("r1"), Phase::Warning);
    }

    /// 存在即计时模式：后台进程常驻使 active 恒真，active 上升沿永不出现。
    /// 监控线程显式传入的前台重现集合必须能重置状态并重新弹窗。
    #[test]
    fn foreground_reappear_triggers_warn_even_when_active_stays_true() {
        let mut bg_rule = rule();
        bg_rule.allow_background = true;
        let rules = vec![bg_rule];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let none = HashSet::new();

        tracker.tick(t0, &used, &rules, INTERVAL, &active(), &none);
        let enforced_at = t0 + Duration::from_secs(15);
        tracker.tick(enforced_at, &used, &rules, INTERVAL, &active(), &none);
        assert_eq!(tracker.phase("r1"), Phase::Enforced);

        // active 仍恒真（后台常驻），但前台窗口重新出现 → 显式边沿
        let fg_reappear: HashSet<String> = HashSet::from(["r1".to_string()]);
        let actions = tracker.tick(
            enforced_at + Duration::from_secs(1),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &fg_reappear,
        );
        assert!(
            matches!(actions.as_slice(), [LimitAction::Warn(id)] if id == "r1"),
            "前台重现必须重新弹窗，即便 active 恒真"
        );
        assert_eq!(tracker.phase("r1"), Phase::Warning);
        // 宽限期从重现时刻重新起算
        let at = enforced_at + Duration::from_secs(1);
        assert!(tracker
            .tick(at + Duration::from_secs(14), &used, &rules, INTERVAL, &active(), &none)
            .is_empty());
        let actions = tracker.tick(
            at + Duration::from_secs(15),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &none,
        );
        assert!(matches!(actions.as_slice(), [LimitAction::Enforce(_)]));
    }

    /// 未开启「允许保留后台」的规则不得被前台重现集合误触发，
    /// 它的重弹仍只走 active 上升沿。
    #[test]
    fn foreground_reappear_ignored_when_background_not_allowed() {
        let rules = vec![rule()];
        let used = used_over();
        let mut tracker = LimitTracker::new();
        let t0 = Instant::now();
        let none = HashSet::new();

        tracker.tick(t0, &used, &rules, INTERVAL, &active(), &none);
        let enforced_at = t0 + Duration::from_secs(15);
        tracker.tick(enforced_at, &used, &rules, INTERVAL, &active(), &none);

        let fg_reappear: HashSet<String> = HashSet::from(["r1".to_string()]);
        let actions = tracker.tick(
            enforced_at + Duration::from_secs(1),
            &used,
            &rules,
            INTERVAL,
            &active(),
            &fg_reappear,
        );
        assert!(actions.is_empty(), "未允许保留后台时不应被显式边沿误触发");
        assert_eq!(tracker.phase("r1"), Phase::Enforced);
    }
}
