use std::collections::{HashMap, HashSet};

use rusqlite::params;
use serde::Serialize;

use crate::db::Db;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
pub struct DayStat {
    pub day: String,
    pub total_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExeStat {
    pub exe_key: String,
    pub exe_name: String,
    pub total_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayCategoryStat {
    pub day: String,
    pub focus_ms: i64,
    pub slack_ms: i64,
    pub focus_slack_ms: i64,
    pub total_ms: i64,
}

/// 生成「区间并集」CTE：按 `group_cols` 分组，把重叠/相邻区间先合并再求长度。
///
/// 直接 `SUM(end_ts - start_ts)` 会把多规则重叠的秒数重复计入，单日可能超过 24 小时。
/// `prefix` 让同一条 SQL 可以并列多份互不冲突的并集 CTE。
fn union_cte(prefix: &str, source: &str, group_cols: &str) -> String {
    format!(
        "{p}_ordered AS (
           SELECT {g}, start_ts, end_ts,
                  MAX(end_ts) OVER (
                    PARTITION BY {g} ORDER BY start_ts, end_ts
                    ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                  ) AS prev_end
           FROM ({source})
         ),
         {p}_flagged AS (
           SELECT {g}, start_ts, end_ts,
                  CASE WHEN prev_end IS NULL OR start_ts > prev_end THEN 1 ELSE 0 END AS is_new
           FROM {p}_ordered
         ),
         {p}_grouped AS (
           SELECT {g}, start_ts, end_ts,
                  SUM(is_new) OVER (
                    PARTITION BY {g} ORDER BY start_ts, end_ts
                    ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
                  ) AS grp
           FROM {p}_flagged
         ),
         {p}_merged AS (
           SELECT {g}, MIN(start_ts) AS s, MAX(end_ts) AS e
           FROM {p}_grouped GROUP BY {g}, grp
         )",
        p = prefix,
        g = group_cols,
        source = source,
    )
}

/// SQL 字符串字面量：单引号翻倍，避免 rule_id 里的特殊字符破坏语句。
fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// 某天每条规则各自用了多久（rule_id -> ms）。额度链路必须保持独立累加。
pub fn used_by_rule(db: &Db, day: &str) -> AppResult<HashMap<String, i64>> {
    let conn = db.reader();
    let mut stmt = conn.prepare(
        "SELECT rule_id, SUM(duration_ms) FROM sessions
         WHERE day = ?1 AND rule_id IS NOT NULL GROUP BY rule_id",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

/// 区间内每天的合计时长：分类区间优先，缺失日期回退到 sessions 区间并集。
pub fn day_stats(db: &Db, from: &str, to: &str) -> AppResult<Vec<DayStat>> {
    let source = "SELECT day, start_ts, end_ts FROM category_sessions
                   WHERE day >= ?1 AND day <= ?2
                   UNION ALL
                   SELECT day, start_ts, end_ts FROM sessions
                   WHERE day >= ?1 AND day <= ?2
                     AND day NOT IN (SELECT DISTINCT day FROM category_sessions)";
    let sql = format!(
        "WITH {cte}
         SELECT day, SUM(e - s) AS total
         FROM u_merged GROUP BY day ORDER BY day",
        cte = union_cte("u", source, "day"),
    );
    let conn = db.reader();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(DayStat {
            day: r.get(0)?,
            total_ms: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 某天按进程拆分的明细；同一进程的重叠区间只算一次。
pub fn day_detail(db: &Db, day: &str) -> AppResult<Vec<ExeStat>> {
    let sql = format!(
        "WITH {cte}
         SELECT exe_key, exe_name, SUM(e - s) AS total
         FROM u_merged GROUP BY exe_key, exe_name ORDER BY total DESC",
        cte = union_cte(
            "u",
            "SELECT exe_key, exe_name, start_ts, end_ts FROM sessions WHERE day = ?1",
            "exe_key, exe_name",
        ),
    );
    let conn = db.reader();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(ExeStat {
            exe_key: r.get(0)?,
            exe_name: r.get(1)?,
            total_ms: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 区间内 Top N 进程；同一进程的重叠区间只算一次。
pub fn top_exes(db: &Db, from: &str, to: &str, limit: usize) -> AppResult<Vec<ExeStat>> {
    let sql = format!(
        "WITH {cte}
         SELECT exe_key, exe_name, SUM(e - s) AS total
         FROM u_merged GROUP BY exe_key, exe_name ORDER BY total DESC LIMIT ?3",
        cte = union_cte(
            "u",
            "SELECT exe_key, exe_name, start_ts, end_ts FROM sessions
             WHERE day >= ?1 AND day <= ?2",
            "exe_key, exe_name",
        ),
    );
    let conn = db.reader();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![from, to, limit as i64], |r| {
        Ok(ExeStat {
            exe_key: r.get(0)?,
            exe_name: r.get(1)?,
            total_ms: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 全部历史总时长：按自然日并集后再求和，避免单日超过 24 小时。
pub fn grand_total_ms(db: &Db) -> AppResult<i64> {
    let stats = day_stats(db, "0000-01-01", "9999-12-31")?;
    Ok(stats.iter().map(|d| d.total_ms).sum())
}

/// 历史分类统计：三类互斥，相加等于并集后的总时长。
///
/// 升级后的日期直接读分类区间；升级前的日期按当前规则类型近似回填：专注规则记为 focus，
/// 其余（含历史规则、设备使用轨道）记为 slack，两段重叠的部分归为 focus_slack，
/// 因此旧数据也只是近似分类，精确口径从升级后的分类时间线开始。
pub fn day_category_stats(
    db: &Db,
    from: &str,
    to: &str,
    focus_rule_ids: &[String],
) -> AppResult<Vec<DayCategoryStat>> {
    let mut out = Vec::new();

    // 分类时间线已落库的日期：三段本来就互斥，按区间并集读回即可。
    {
        let source = "SELECT day, category, start_ts, end_ts FROM category_sessions
                       WHERE day >= ?1 AND day <= ?2";
        let sql = format!(
            "WITH {cat_cte}, {total_cte}
             SELECT t.day,
                    COALESCE(MAX(CASE WHEN c.category = 'focus' THEN c.total END), 0),
                    COALESCE(MAX(CASE WHEN c.category = 'slack' THEN c.total END), 0),
                    COALESCE(MAX(CASE WHEN c.category = 'focus_slack' THEN c.total END), 0),
                    t.total
             FROM (SELECT day, SUM(e - s) AS total FROM total_merged GROUP BY day) t
             LEFT JOIN (
               SELECT day, category, SUM(e - s) AS total
               FROM cat_merged GROUP BY day, category
             ) c ON c.day = t.day
             GROUP BY t.day, t.total
             ORDER BY t.day",
            cat_cte = union_cte("cat", source, "day, category"),
            total_cte = union_cte("total", source, "day"),
        );
        let conn = db.reader();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok(DayCategoryStat {
                day: r.get(0)?,
                focus_ms: r.get(1)?,
                slack_ms: r.get(2)?,
                focus_slack_ms: r.get(3)?,
                total_ms: r.get(4)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }

    // 升级前的日期：按当前规则类型近似分类。focus ∪ slack 与各自并集的差就是重叠时长，
    // 把它划入 focus_slack，保证三类依旧互斥且相加等于当日总时长。
    {
        let focus_set: HashSet<&str> = focus_rule_ids.iter().map(String::as_str).collect();
        let category_expr = if focus_set.is_empty() {
            "'slack'".to_string()
        } else {
            let ids = focus_rule_ids
                .iter()
                .map(|id| sql_literal(id))
                .collect::<Vec<_>>()
                .join(", ");
            format!("CASE WHEN s.rule_id IN ({ids}) THEN 'focus' ELSE 'slack' END")
        };
        let source = format!(
            "SELECT s.day, {category_expr} AS category, s.start_ts, s.end_ts
               FROM sessions s
              WHERE s.day >= ?1 AND s.day <= ?2
                AND s.day NOT IN (SELECT DISTINCT day FROM category_sessions)"
        );
        let sql = format!(
            "WITH {cat_cte}, {total_cte},
             per_day AS (
               SELECT t.day,
                      COALESCE(MAX(CASE WHEN c.category = 'focus' THEN c.total END), 0) AS focus_union,
                      COALESCE(MAX(CASE WHEN c.category = 'slack' THEN c.total END), 0) AS slack_union,
                      t.total
               FROM (SELECT day, SUM(e - s) AS total FROM total_merged GROUP BY day) t
               LEFT JOIN (
                 SELECT day, category, SUM(e - s) AS total
                 FROM cat_merged GROUP BY day, category
               ) c ON c.day = t.day
               GROUP BY t.day, t.total
             )
             SELECT day,
                    focus_union - MAX(0, focus_union + slack_union - total),
                    slack_union - MAX(0, focus_union + slack_union - total),
                    MAX(0, focus_union + slack_union - total),
                    total
             FROM per_day ORDER BY day",
            cat_cte = union_cte("cat", &source, "day, category"),
            total_cte = union_cte("total", &source, "day"),
        );
        let conn = db.reader();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok(DayCategoryStat {
                day: r.get(0)?,
                focus_ms: r.get(1)?,
                slack_ms: r.get(2)?,
                focus_slack_ms: r.get(3)?,
                total_ms: r.get(4)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }

    out.sort_by(|a, b| a.day.cmp(&b.day));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ClosedSeg, Db};
    use crate::monitor::category_timeline::CategorySeg;

    fn temp_db(name: &str) -> (Db, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("florime-query-{}-{}.db", std::process::id(), name));
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.as_os_str().to_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(std::path::Path::new(&p));
        }
        let db = Db::open(&path).expect("open test db");
        (db, path)
    }

    fn seg(rule_id: Option<&str>, start: i64, end: i64) -> ClosedSeg {
        ClosedSeg {
            rule_id: rule_id.map(str::to_string),
            exe_key: "exe".into(),
            exe_name: "exe.exe".into(),
            window_title: None,
            pid: 1,
            mode: "foreground".into(),
            day: "2026-01-02".into(),
            start_ts: start,
            end_ts: end,
            duration_ms: end - start,
            integrity: "ok".into(),
        }
    }

    fn cat(category: &str, start: i64, end: i64) -> CategorySeg {
        CategorySeg {
            day: "2026-01-02".into(),
            category: category.into(),
            start_ts: start,
            end_ts: end,
            duration_ms: end - start,
        }
    }

    #[test]
    fn day_stats_counts_overlapping_rules_once() {
        let (db, path) = temp_db("overlap");
        db.write_batch(&[
            seg(Some("a"), 0, 1_000),
            seg(Some("b"), 500, 1_500),
            seg(Some("a"), 1_400, 2_000),
        ])
        .unwrap();
        let stats = day_stats(&db, "2026-01-01", "2026-01-03").unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].total_ms, 2_000);
        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_stats_are_exclusive_and_sum_to_total() {
        let (db, path) = temp_db("category");
        db.write_category_batch(&[
            cat("focus", 0, 1_000),
            cat("slack", 1_000, 2_000),
            cat("focus_slack", 2_000, 3_000),
        ])
        .unwrap();
        let rows = day_category_stats(&db, "2026-01-01", "2026-01-03", &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].focus_ms, 1_000);
        assert_eq!(rows[0].slack_ms, 1_000);
        assert_eq!(rows[0].focus_slack_ms, 1_000);
        assert_eq!(rows[0].total_ms, 3_000);
        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_stats_fall_back_to_current_rule_kind() {
        let (db, path) = temp_db("fallback");
        db.write_batch(&[
            seg(Some("focus-rule"), 0, 1_000),
            seg(Some("slack-rule"), 500, 1_500),
        ])
        .unwrap();
        let rows = day_category_stats(&db, "2026-01-01", "2026-01-03", &["focus-rule".to_string()])
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].focus_ms, 500);
        assert_eq!(rows[0].slack_ms, 500);
        assert_eq!(rows[0].focus_slack_ms, 500);
        assert_eq!(rows[0].total_ms, 1_500);
        drop(db);
        let _ = std::fs::remove_file(&path);
    }
}
