pub mod queries;

use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;

use chrono::Local;
use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::monitor::category_timeline::CategorySeg;

/// 一段已闭合的计时记录（落库单元）。
#[derive(Debug, Clone)]
pub struct ClosedSeg {
    pub rule_id: Option<String>,
    pub exe_key: String,
    pub exe_name: String,
    pub window_title: Option<String>,
    pub pid: u32,
    pub mode: String,
    pub day: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub duration_ms: i64,
    pub integrity: String,
}

/// SQLite 封装。
///
/// 内置两条连接：写连接只给监控线程用（几乎无竞争），读连接给前端查询用。
/// 两条连接在同一 WAL 库上互不阻塞。
pub struct Db {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  rule_id      TEXT,
  exe_key      TEXT NOT NULL,
  exe_name     TEXT NOT NULL,
  window_title TEXT,
  pid          INTEGER NOT NULL,
  mode         TEXT NOT NULL CHECK (mode IN ('foreground','running')),
  day          TEXT NOT NULL,
  start_ts     INTEGER NOT NULL,
  end_ts       INTEGER NOT NULL,
  duration_ms  INTEGER NOT NULL CHECK (duration_ms >= 0),
  integrity    TEXT NOT NULL DEFAULT 'ok'
);
CREATE INDEX IF NOT EXISTS idx_sessions_day ON sessions(day);
CREATE INDEX IF NOT EXISTS idx_sessions_rule_day ON sessions(rule_id, day);
CREATE INDEX IF NOT EXISTS idx_sessions_exe_day ON sessions(exe_key, day);

CREATE TABLE IF NOT EXISTS daily_rollup (
  day        TEXT NOT NULL,
  exe_key    TEXT NOT NULL,
  rule_id    TEXT,
  mode       TEXT NOT NULL,
  total_ms   INTEGER NOT NULL DEFAULT 0,
  first_ts   INTEGER,
  last_ts    INTEGER,
  updated_ts INTEGER NOT NULL,
  PRIMARY KEY (day, exe_key, mode)
);

CREATE TABLE IF NOT EXISTS category_sessions (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  day        TEXT NOT NULL,
  category   TEXT NOT NULL CHECK (category IN ('focus','slack','focus_slack')),
  start_ts   INTEGER NOT NULL,
  end_ts     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_category_sessions_day ON category_sessions(day);
CREATE INDEX IF NOT EXISTS idx_category_sessions_range ON category_sessions(day, start_ts, end_ts);

CREATE TABLE IF NOT EXISTS interventions (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  ts        INTEGER NOT NULL,
  rule_id   TEXT,
  exe_key   TEXT NOT NULL,
  pid       INTEGER,
  stage     TEXT NOT NULL,
  result    TEXT,
  win_error INTEGER
);
CREATE INDEX IF NOT EXISTS idx_interventions_ts ON interventions(ts);

CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
"#;

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

fn backup_before_migration(path: &Path) -> AppResult<()> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S%3f").to_string();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let src = Path::new(&name);
        if !src.exists() {
            continue;
        }
        let mut dst_name = name.clone();
        dst_name.push(OsString::from(format!(".backup-{stamp}")));
        std::fs::copy(src, Path::new(&dst_name))
            .map_err(|e| AppError::System(format!("备份数据库失败 {}: {e}", src.display())))?;
        log::info!("数据库迁移前已备份 {}", Path::new(&dst_name).display());
    }
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        // 全新空库不需要备份；只在确实已有数据时才为迁移留快照
        let had_data = path.metadata().map(|m| m.len() > 0).unwrap_or(false);
        let writer = Connection::open(path)?;
        configure(&writer)?;
        let version: u32 = writer.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if had_data && version < 2 {
            backup_before_migration(path)?;
        }
        writer.execute_batch(SCHEMA)?;
        writer.pragma_update(None, "user_version", 2)?;

        let reader = Connection::open(path)?;
        configure(&reader)?;

        Ok(Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        })
    }

    /// 批量落库：一次事务写多行 session，并增量更新 rollup。
    pub fn write_batch(&self, segs: &[ClosedSeg]) -> AppResult<()> {
        if segs.is_empty() {
            return Ok(());
        }
        let mut conn = self.writer.lock().expect("db writer poisoned");
        let tx = conn.transaction()?;
        {
            let mut ins = tx.prepare_cached(
                "INSERT INTO sessions
                 (rule_id, exe_key, exe_name, window_title, pid, mode, day, start_ts, end_ts, duration_ms, integrity)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?;
            let mut roll = tx.prepare_cached(
                "INSERT INTO daily_rollup (day, exe_key, rule_id, mode, total_ms, first_ts, last_ts, updated_ts)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(day, exe_key, mode) DO UPDATE SET
                   total_ms = total_ms + excluded.total_ms,
                   first_ts = MIN(COALESCE(first_ts, excluded.first_ts), excluded.first_ts),
                   last_ts  = MAX(COALESCE(last_ts, excluded.last_ts), excluded.last_ts),
                   updated_ts = excluded.updated_ts",
            )?;
            for seg in segs {
                ins.execute(params![
                    seg.rule_id,
                    seg.exe_key,
                    seg.exe_name,
                    seg.window_title,
                    seg.pid,
                    seg.mode,
                    seg.day,
                    seg.start_ts,
                    seg.end_ts,
                    seg.duration_ms,
                    seg.integrity,
                ])?;
                roll.execute(params![
                    seg.day,
                    seg.exe_key,
                    seg.rule_id,
                    seg.mode,
                    seg.duration_ms,
                    seg.start_ts,
                    seg.end_ts,
                    seg.end_ts,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 分类区间是今日累计与历史统计的去重数据源，不参与额度计算。
    pub fn write_category_batch(&self, segs: &[CategorySeg]) -> AppResult<()> {
        if segs.is_empty() {
            return Ok(());
        }
        let mut conn = self.writer.lock().expect("db writer poisoned");
        let tx = conn.transaction()?;
        {
            let mut ins = tx.prepare_cached(
                "INSERT INTO category_sessions (day, category, start_ts, end_ts)
                 VALUES (?1,?2,?3,?4)",
            )?;
            for seg in segs {
                ins.execute(params![seg.day, seg.category, seg.start_ts, seg.end_ts])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 记录一次干预行为，用于审计与排障。
    pub fn record_intervention(
        &self,
        rule_id: Option<&str>,
        exe_key: &str,
        pid: u32,
        stage: &str,
        result: &str,
        win_error: Option<i64>,
    ) {
        let conn = self.writer.lock().expect("db writer poisoned");
        let ts = chrono::Utc::now().timestamp_millis();
        if let Err(e) = conn.execute(
            "INSERT INTO interventions (ts, rule_id, exe_key, pid, stage, result, win_error)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![ts, rule_id, exe_key, pid, stage, result, win_error],
        ) {
            log::warn!("写入干预记录失败: {e}");
        }
    }

    /// 清理过期明细（rollup 保留聚合）。
    pub fn prune(&self, retention_days: u32) -> AppResult<usize> {
        let conn = self.writer.lock().expect("db writer poisoned");
        let cutoff = (chrono::Local::now() - chrono::Duration::days(retention_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let n = conn.execute("DELETE FROM sessions WHERE day < ?1", params![cutoff])?;
        conn.execute(
            "DELETE FROM interventions WHERE ts < ?1",
            params![
                (chrono::Utc::now().timestamp_millis() - i64::from(retention_days) * 86_400_000)
            ],
        )?;
        Ok(n)
    }

    pub(crate) fn reader(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.reader.lock().expect("db reader poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_db_is_backed_up_before_version_upgrade() {
        let dir = std::env::temp_dir().join(format!(
            "florime-migrate-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_millis()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("focus.db");

        {
            let db = Db::open(&path).unwrap();
            drop(db);
        }
        // 模拟一个 user_version = 1 的旧库
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
        }
        {
            let db = Db::open(&path).unwrap();
            let version: u32 = db
                .writer
                .lock()
                .unwrap()
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .unwrap();
            assert_eq!(version, 2);
        }

        let backups: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".backup-"))
            .collect();
        assert!(
            backups.iter().any(|n| n.starts_with("focus.db.backup-")),
            "迁移前应备份主库, 实际: {backups:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
