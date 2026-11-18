//! 计时积分器。
//!
//! 正确性靠三件事：
//! 1. `Instant`（QPC）算间隔，不用墙钟相减；
//! 2. `dt > max_gap` 判为休眠/卡顿，本 tick 记 0 并强制闭合；
//! 3. 段结束时刻用 `last_seen` 而不是 `now`，避免尾段多吃一个 interval。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDate, TimeZone};

use crate::db::ClosedSeg;

/// 墙钟与单调时钟之差的容忍值。超过即判定为休眠恢复或人为改表，本段不计入。
const CLOCK_DRIFT_TOLERANCE_MS: i64 = 5_000;

/// 本 tick 处于"正在使用"状态的一个（规则, 应用）组合。
#[derive(Clone, Debug)]
pub struct ActiveApp {
    /// `Some` = 规则计时，计入额度；`None` = 设备使用轨道，只进统计
    pub rule_id: Option<String>,
    pub exe_key: String,
    pub exe_name: String,
    pub pid: u32,
    pub mode: String,
    pub title: String,
    pub integrity: String,
}

/// 未闭合段的键：规则轨道按规则 id，使用轨道按进程键。
///
/// 两种轨道共存于同一张表，加前缀避免规则 id 与路径撞名。
fn track_key(a: &ActiveApp) -> String {
    match &a.rule_id {
        Some(id) => format!("rule:{id}"),
        None => format!("use:{}", a.exe_key),
    }
}

struct OpenSeg {
    app: ActiveApp,
    started_mono: Instant,
    started_wall: DateTime<Local>,
    last_seen_mono: Instant,
    day: NaiveDate,
}

pub struct Integrator {
    open: HashMap<String, OpenSeg>,
    last_tick: Option<Instant>,
    /// 上一 tick 的墙钟，与 `last_tick` 配对用于识别时钟跳变
    last_wall: Option<DateTime<Local>>,
    cur_day: NaiveDate,
}

/// 本地时区的某天 00:00。DST 歧义时取较早的那个。
fn local_midnight(day: NaiveDate) -> Option<DateTime<Local>> {
    let naive = day.and_hms_opt(0, 0, 0)?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
}

impl Integrator {
    pub fn new(today: NaiveDate) -> Self {
        Self {
            open: HashMap::new(),
            last_tick: None,
            last_wall: None,
            cur_day: today,
        }
    }

    /// 某规则当前这个未闭合段已经持续多久。
    pub fn open_ms(&self, rule_id: &str, now: Instant) -> u64 {
        self.open
            .get(&format!("rule:{rule_id}"))
            .map(|s| now.saturating_duration_since(s.started_mono).as_millis() as u64)
            .unwrap_or(0)
    }

    /// 推进一个 tick，返回本次闭合的所有段。
    pub fn step(
        &mut self,
        now: Instant,
        wall: DateTime<Local>,
        active: &[ActiveApp],
        max_gap: Duration,
    ) -> Vec<ClosedSeg> {
        let mut out = Vec::new();
        let today = wall.date_naive();

        // 休眠 / 卡顿 / 人为改表：这一段空白不可信，一律按 last_seen 闭合，绝不把空白计入用量
        let mono_delta = self
            .last_tick
            .map(|t| now.saturating_duration_since(t))
            .unwrap_or_default();
        let wall_delta = self
            .last_wall
            .map(|w| wall.signed_duration_since(w).num_milliseconds())
            .unwrap_or_default();

        let stale = self.last_tick.is_some() && mono_delta > max_gap;
        let drift = self.last_wall.is_some()
            && (wall_delta - mono_delta.as_millis() as i64).abs() > CLOCK_DRIFT_TOLERANCE_MS;

        if stale || drift {
            if drift {
                log::warn!(
                    "墙钟与单调时钟偏差 {wall_delta}ms（单调 {:?}），本段按 last_seen 闭合",
                    mono_delta
                );
            } else {
                log::info!("计时间隙 {:?}，本段按 last_seen 闭合", mono_delta);
            }
            let keys: Vec<String> = self.open.keys().cloned().collect();
            for k in keys {
                self.close_one(&k, now, true, &mut out);
            }
        }

        // 跨天：只跟随确实前进的日期；时钟回拨保留原当日额度
        if today != self.cur_day {
            if today > self.cur_day {
                self.rollover(now, wall, &mut out);
                self.cur_day = today;
            } else {
                log::warn!(
                    "检测到时钟回拨 {} → {}，当日额度不重置",
                    self.cur_day,
                    today
                );
            }
        }

        // 不在活跃集合里的段闭合
        let active_ids: std::collections::HashSet<String> = active.iter().map(track_key).collect();
        let stale_keys: Vec<String> = self
            .open
            .keys()
            .filter(|k| !active_ids.contains(k.as_str()))
            .cloned()
            .collect();
        for k in stale_keys {
            self.close_one(&k, now, true, &mut out);
        }

        // 新活跃的段打开 / 已活跃的刷新 last_seen
        for a in active {
            let key = track_key(a);
            if let Some(seg) = self.open.get_mut(&key) {
                seg.last_seen_mono = now;
                seg.app.title = a.title.clone();
                seg.app.pid = a.pid;
            } else {
                self.open.insert(
                    key,
                    OpenSeg {
                        app: a.clone(),
                        started_mono: now,
                        started_wall: wall,
                        last_seen_mono: now,
                        day: today,
                    },
                );
            }
        }

        self.last_tick = Some(now);
        self.last_wall = Some(wall);
        out
    }

    /// 跨天：把正在进行的段在本地 00:00 处切开。
    fn rollover(&mut self, now: Instant, wall: DateTime<Local>, out: &mut Vec<ClosedSeg>) {
        let Some(midnight) = local_midnight(wall.date_naive()) else {
            let keys: Vec<String> = self.open.keys().cloned().collect();
            for k in keys {
                self.close_one(&k, now, true, out);
            }
            return;
        };
        let keys: Vec<String> = self.open.keys().cloned().collect();
        for k in keys {
            let Some(seg) = self.open.remove(&k) else {
                continue;
            };
            let to_mid = midnight.signed_duration_since(seg.started_wall);
            if to_mid.num_milliseconds() <= 0 {
                // 段起点就在今天，原样恢复
                self.open.insert(k, seg);
                continue;
            }
            let dur_ms = to_mid.num_milliseconds();
            out.push(ClosedSeg {
                rule_id: seg.app.rule_id.clone(),
                exe_key: seg.app.exe_key.clone(),
                exe_name: seg.app.exe_name.clone(),
                window_title: Some(seg.app.title.clone()),
                pid: seg.app.pid,
                mode: seg.app.mode.clone(),
                day: seg.day.format("%Y-%m-%d").to_string(),
                start_ts: seg.started_wall.timestamp_millis(),
                end_ts: midnight.timestamp_millis(),
                duration_ms: dur_ms,
                integrity: seg.app.integrity.clone(),
            });
            // 新段从 midnight 开始
            let elapsed_after = wall.signed_duration_since(midnight).num_milliseconds().max(0);
            let started_mono = now
                .checked_sub(Duration::from_millis(elapsed_after as u64))
                .unwrap_or(now);
            self.open.insert(
                k,
                OpenSeg {
                    app: seg.app,
                    started_mono,
                    started_wall: midnight,
                    last_seen_mono: now,
                    day: wall.date_naive(),
                },
            );
        }
    }

    fn close_one(
        &mut self,
        rule_id: &str,
        now: Instant,
        use_last_seen: bool,
        out: &mut Vec<ClosedSeg>,
    ) {
        let Some(seg) = self.open.remove(rule_id) else {
            return;
        };
        let end_mono = if use_last_seen {
            seg.last_seen_mono
        } else {
            now
        };
        let dur = end_mono.saturating_duration_since(seg.started_mono);
        let dur_ms = dur.as_millis() as i64;
        if dur_ms < 200 {
            // 太短的抖动不落库：切窗口时会产生大量 <200ms 的碎片
            return;
        }
        let end_wall = seg.started_wall + dur;
        out.push(ClosedSeg {
            rule_id: seg.app.rule_id.clone(),
            exe_key: seg.app.exe_key.clone(),
            exe_name: seg.app.exe_name.clone(),
            window_title: Some(seg.app.title.clone()),
            pid: seg.app.pid,
            mode: seg.app.mode.clone(),
            day: seg.day.format("%Y-%m-%d").to_string(),
            start_ts: seg.started_wall.timestamp_millis(),
            end_ts: end_wall.timestamp_millis(),
            duration_ms: dur_ms,
            integrity: seg.app.integrity.clone(),
        });
    }

    /// 关闭全部（暂停 / 退出 / 配置变更时调用）。
    pub fn close_all(&mut self, now: Instant, out: &mut Vec<ClosedSeg>) {
        let keys: Vec<String> = self.open.keys().cloned().collect();
        for k in keys {
            self.close_one(&k, now, true, out);
        }
        // 关闭后重新起算，避免下一次 tick 把这段静止时间当成间隙
        self.last_tick = Some(now);
        self.last_wall = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    const MAX_GAP: Duration = Duration::from_secs(5);

    fn wall_at(hour: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 18, hour, 0, 0)
            .unwrap()
    }

    fn app(rule_id: &str) -> ActiveApp {
        ActiveApp {
            rule_id: Some(rule_id.to_string()),
            exe_key: "c:\\test\\game.exe".into(),
            exe_name: "game.exe".into(),
            pid: 4242,
            mode: "foreground".into(),
            title: "测试窗口".into(),
            integrity: "ok".into(),
        }
    }

    /// 设备使用轨道：无规则归属，落库时 rule_id 为空
    fn usage_app(exe: &str) -> ActiveApp {
        ActiveApp {
            rule_id: None,
            exe_key: format!("c:\\test\\{exe}"),
            exe_name: exe.into(),
            pid: 100,
            mode: "foreground".into(),
            title: String::new(),
            integrity: "ok".into(),
        }
    }

    #[test]
    fn usage_track_closes_without_rule_id() {
        let mut it = Integrator::new(wall_at(9).date_naive());
        let t0 = Instant::now();

        it.step(t0, wall_at(9), &[usage_app("chrome.exe")], MAX_GAP);
        it.step(
            t0 + Duration::from_secs(1),
            wall_at(9) + ChronoDuration::seconds(1),
            &[usage_app("chrome.exe")],
            MAX_GAP,
        );
        // 段通常按 last_seen 闭合：切换前最后一次见到 chrome 是 +1s
        let out = it.step(
            t0 + Duration::from_secs(2),
            wall_at(9) + ChronoDuration::seconds(2),
            &[usage_app("code.exe")],
            MAX_GAP,
        );

        assert_eq!(out.len(), 1);
        assert!(out[0].rule_id.is_none(), "使用轨道不应带规则 id");
        assert_eq!(out[0].exe_name, "chrome.exe");
        assert_eq!(out[0].duration_ms, 1_000);
    }

    #[test]
    fn sleep_gap_is_not_counted() {
        let mut it = Integrator::new(wall_at(9).date_naive());
        let t0 = Instant::now();

        it.step(t0, wall_at(9), &[app("r1")], MAX_GAP);
        let t1 = t0 + Duration::from_secs(1);
        assert!(it
            .step(t1, wall_at(9) + ChronoDuration::seconds(1), &[app("r1")], MAX_GAP)
            .is_empty());

        // 合盖 8 小时：只在 last_seen(t1) 处闭合，1 秒的用量不该被放大成 8 小时
        let t2 = t1 + Duration::from_secs(8 * 3600);
        let out = it.step(
            t2,
            wall_at(9) + ChronoDuration::seconds(8 * 3600 + 1),
            &[app("r1")],
            MAX_GAP,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].duration_ms, 1_000);
    }

    #[test]
    fn clock_jump_is_closed_at_last_seen() {
        let mut it = Integrator::new(wall_at(9).date_naive());
        let t0 = Instant::now();

        it.step(t0, wall_at(9), &[app("r1")], MAX_GAP);
        let t1 = t0 + Duration::from_secs(1);
        it.step(t1, wall_at(9) + ChronoDuration::seconds(1), &[app("r1")], MAX_GAP);

        // 单调时钟只走了 1 秒，墙钟却被拨快 1 小时：按 last_seen 闭合，不跨天不放大
        let t2 = t1 + Duration::from_secs(1);
        let out = it.step(
            t2,
            wall_at(9) + ChronoDuration::seconds(1) + ChronoDuration::hours(1),
            &[app("r1")],
            MAX_GAP,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].duration_ms, 1_000);
    }

    #[test]
    fn clock_rollback_keeps_current_day() {
        let mut it = Integrator::new(wall_at(9).date_naive());
        let t0 = Instant::now();
        it.step(t0, wall_at(9), &[app("r1")], MAX_GAP);

        // 墙钟回拨到前一天：不应触发跨天切分
        let prev = wall_at(9) - ChronoDuration::days(1) + ChronoDuration::seconds(1);
        let out = it.step(t0 + Duration::from_secs(1), prev, &[app("r1")], MAX_GAP);
        assert!(out.is_empty());
        assert_eq!(it.cur_day, wall_at(9).date_naive());
    }
}
