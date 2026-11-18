//! 今日累计分类时间线。
//!
//! 与 per-rule 积分器并行工作，只关心「当前是否有专注/摸鱼规则命中」：
//! - 仅专注 → Focus
//! - 仅摸鱼 → Slack
//! - 两者同时 → FocusSlack
//!
//! 该时间线只追加、闭合区间，不因为规则状态切换重写历史区间。

use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;

const CLOCK_DRIFT_TOLERANCE_MS: i64 = 5_000;
const MIN_SEG_MS: i64 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeCategory {
    Focus,
    Slack,
    FocusSlack,
}

impl TimeCategory {
    pub fn from_flags(has_focus: bool, has_slack: bool) -> Option<Self> {
        match (has_focus, has_slack) {
            (true, true) => Some(Self::FocusSlack),
            (true, false) => Some(Self::Focus),
            (false, true) => Some(Self::Slack),
            (false, false) => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Slack => "slack",
            Self::FocusSlack => "focus_slack",
        }
    }
}

/// 已闭合的分类区间，落库到 `category_sessions`。
#[derive(Clone, Debug, Serialize)]
pub struct CategorySeg {
    pub day: String,
    pub category: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub duration_ms: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CategoryTotals {
    pub total_ms: u64,
    pub focus_ms: u64,
    pub slack_ms: u64,
    pub focus_slack_ms: u64,
}

struct OpenSpan {
    category: TimeCategory,
    started_mono: Instant,
    started_wall: DateTime<Local>,
    last_seen_mono: Instant,
    last_wall: DateTime<Local>,
    day: NaiveDate,
}

pub struct CategoryTimeline {
    open: Option<OpenSpan>,
    last_tick: Option<Instant>,
    last_wall: Option<DateTime<Local>>,
    cur_day: NaiveDate,
    baseline: CategoryTotals,
    today_segs: Vec<CategorySeg>,
}

impl CategoryTimeline {
    pub fn new(today: NaiveDate) -> Self {
        Self {
            open: None,
            last_tick: None,
            last_wall: None,
            cur_day: today,
            baseline: CategoryTotals::default(),
            today_segs: Vec::new(),
        }
    }

    pub fn set_baseline(&mut self, totals: CategoryTotals) {
        self.baseline = totals;
    }

    pub fn reset_day(&mut self, day: NaiveDate) {
        self.cur_day = day;
        self.baseline = CategoryTotals::default();
        self.today_segs.clear();
    }

    /// 推进一个 tick，返回本次闭合的区间。
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        now: Instant,
        wall: DateTime<Local>,
        has_focus: bool,
        has_slack: bool,
        max_gap: Duration,
    ) -> Vec<CategorySeg> {
        let mut out = Vec::new();
        let today = wall.date_naive();
        let today_key = today.format("%Y-%m-%d").to_string();

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
            if let Some(seg) = self.close_current() {
                out.push(seg);
            }
            self.last_wall = None;
        }

        if today > self.cur_day {
            if let Some(seg) = self.close_current() {
                out.push(seg);
            }
            self.reset_day(today);
        } else if today < self.cur_day {
            log::warn!("检测到时钟回拨，分类计时仍归入原自然日");
        }

        let next = TimeCategory::from_flags(has_focus, has_slack);
        let changed = match (&self.open, next) {
            (Some(open), Some(next_cat)) => open.category != next_cat || open.day != today,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        if changed {
            if let Some(seg) = self.close_current() {
                out.push(seg);
            }
            if let Some(category) = next {
                self.open = Some(OpenSpan {
                    category,
                    started_mono: now,
                    started_wall: wall,
                    last_seen_mono: now,
                    last_wall: wall,
                    day: today,
                });
            }
        } else if let Some(open) = self.open.as_mut() {
            open.last_seen_mono = now;
            open.last_wall = wall;
        }

        // 跨天闭合的区间属于昨天，只落库、不计入今天的累计
        for seg in &out {
            if seg.day == today_key {
                self.today_segs.push(seg.clone());
            }
        }
        self.last_tick = Some(now);
        self.last_wall = Some(wall);
        out
    }

    /// 暂停、休息、规则切换、退出时闭合当前区间。
    pub fn close_all(&mut self) -> Vec<CategorySeg> {
        let seg = self.close_current();
        self.last_tick = None;
        self.last_wall = None;
        seg.into_iter().collect()
    }

    /// 按操作时刻闭合当前区间：已累计的时长不回退，也不会把暂停前的一小段丢掉。
    /// 墙钟跨天时不延长，避免把次日的时间错误归到前一天。
    pub fn close_all_at(&mut self, now: Instant, wall: DateTime<Local>) -> Vec<CategorySeg> {
        let same_day = self
            .open
            .as_ref()
            .is_some_and(|open| open.day == wall.date_naive());
        if same_day {
            if let Some(open) = self.open.as_mut() {
                if now > open.last_seen_mono {
                    open.last_seen_mono = now;
                }
            }
        }
        self.close_all()
    }

    fn close_current(&mut self) -> Option<CategorySeg> {
        let open = self.open.take()?;
        let dur = open
            .last_seen_mono
            .saturating_duration_since(open.started_mono);
        let dur_ms = dur.as_millis() as i64;
        if dur_ms < MIN_SEG_MS {
            return None;
        }
        let end_wall = open.started_wall + dur;
        let seg = CategorySeg {
            day: open.day.format("%Y-%m-%d").to_string(),
            category: open.category.as_str().to_string(),
            start_ts: open.started_wall.timestamp_millis(),
            end_ts: end_wall.timestamp_millis(),
            duration_ms: dur_ms,
        };
        Some(seg)
    }

    pub fn totals(&self) -> CategoryTotals {
        let mut focus = Vec::new();
        let mut slack = Vec::new();
        let mut focus_slack = Vec::new();
        let mut all = Vec::new();

        let mut add = |seg: &CategorySeg, end_ts: i64| {
            if end_ts <= seg.start_ts {
                return;
            }
            all.push((seg.start_ts, end_ts));
            match seg.category.as_str() {
                "focus" => focus.push((seg.start_ts, end_ts)),
                "slack" => slack.push((seg.start_ts, end_ts)),
                "focus_slack" => focus_slack.push((seg.start_ts, end_ts)),
                _ => {}
            }
        };

        for seg in &self.today_segs {
            add(seg, seg.end_ts);
        }
        if let Some(open) = &self.open {
            let end_ts = open.last_wall.timestamp_millis();
            let seg = CategorySeg {
                day: open.day.format("%Y-%m-%d").to_string(),
                category: open.category.as_str().to_string(),
                start_ts: open.started_wall.timestamp_millis(),
                end_ts,
                duration_ms: end_ts - open.started_wall.timestamp_millis(),
            };
            add(&seg, end_ts);
        }

        CategoryTotals {
            total_ms: self.baseline.total_ms.saturating_add(union_ms(all)),
            focus_ms: self.baseline.focus_ms.saturating_add(union_ms(focus)),
            slack_ms: self.baseline.slack_ms.saturating_add(union_ms(slack)),
            focus_slack_ms: self
                .baseline
                .focus_slack_ms
                .saturating_add(union_ms(focus_slack)),
        }
    }
}

/// 半开区间的并集长度；重叠只计一次。
pub fn union_ms(mut spans: Vec<(i64, i64)>) -> u64 {
    spans.retain(|(s, e)| e > s);
    if spans.is_empty() {
        return 0;
    }
    spans.sort_unstable_by_key(|s| (s.0, s.1));
    let (mut cur_s, mut cur_e) = spans[0];
    let mut total: i64 = 0;
    for (s, e) in spans.into_iter().skip(1) {
        if s > cur_e {
            total += cur_e - cur_s;
            cur_s = s;
            cur_e = e;
        } else if e > cur_e {
            cur_e = e;
        }
    }
    total += cur_e - cur_s;
    total.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_handles_overlap() {
        assert_eq!(union_ms(vec![(0, 10), (5, 15), (20, 25)]), 20);
    }

    #[test]
    fn restored_totals_are_included_and_reset_on_new_day() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        let mut timeline = CategoryTimeline::new(today);
        timeline.set_baseline(CategoryTotals {
            total_ms: 3_000,
            focus_ms: 1_000,
            slack_ms: 1_500,
            focus_slack_ms: 500,
        });

        assert_eq!(timeline.totals().total_ms, 3_000);
        assert_eq!(timeline.totals().focus_ms, 1_000);

        timeline.reset_day(today.succ_opt().unwrap());
        assert_eq!(timeline.totals().total_ms, 0);
    }
}
