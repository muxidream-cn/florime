//! 进程快照（Toolhelp32）与进程树遍历。
//!
//! 用 Toolhelp32 而不是 sysinfo：一次快照顺带拿到 pid、父 pid、映像名，
//! 正好覆盖"存在性检测"和"进程树展开"两个需求，零额外依赖。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

#[derive(Debug, Clone)]
pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    /// 原始大小写
    pub exe_name: String,
}

struct Cache {
    taken: Instant,
    entries: Vec<ProcEntry>,
}

fn cache() -> &'static Mutex<Option<Cache>> {
    static C: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// 取全部进程快照（默认缓存 2 秒）。
pub fn snapshot(ttl: Duration) -> Vec<ProcEntry> {
    {
        let guard = cache().lock().expect("proc cache poisoned");
        if let Some(c) = guard.as_ref() {
            if c.taken.elapsed() < ttl {
                return c.entries.clone();
            }
        }
    }

    let entries = take_snapshot();
    let mut guard = cache().lock().expect("proc cache poisoned");
    *guard = Some(Cache {
        taken: Instant::now(),
        entries: entries.clone(),
    });
    entries
}

/// 强制刷新（写操作前用，避免拿到过期数据）。
pub fn snapshot_fresh() -> Vec<ProcEntry> {
    snapshot(Duration::ZERO)
}

fn take_snapshot() -> Vec<ProcEntry> {
    let mut out = Vec::with_capacity(256);
    unsafe {
        let snap: HANDLE = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("CreateToolhelp32Snapshot 失败: {e}");
                return out;
            }
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                out.push(ProcEntry {
                    pid: entry.th32ProcessID,
                    ppid: entry.th32ParentProcessID,
                    exe_name: name,
                });
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// pid -> 映像名（用于 OpenProcess 失败时的降级）。
pub fn find_name(pid: u32) -> Option<String> {
    snapshot(Duration::from_secs(2))
        .into_iter()
        .find(|e| e.pid == pid)
        .map(|e| e.exe_name)
}

/// 是否存在该 pid。
pub fn exists(pid: u32) -> bool {
    snapshot(Duration::from_secs(2)).iter().any(|e| e.pid == pid)
}

/// 某 exe 名当前有多少个实例在跑。
pub fn count_by_name(exe_name: &str) -> usize {
    let target = exe_name.to_lowercase();
    snapshot(Duration::from_secs(2))
        .iter()
        .filter(|e| e.exe_name.to_lowercase() == target)
        .count()
}

/// 展开某进程的全部后代（不含自身），用于用户显式触发的强制关闭进程树。
pub fn descendants_of(pid: u32) -> Vec<u32> {
    let entries = snapshot_fresh();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for e in &entries {
        children.entry(e.ppid).or_default().push(e.pid);
    }
    let mut out = Vec::new();
    let mut stack = vec![pid];
    while let Some(cur) = stack.pop() {
        if let Some(kids) = children.get(&cur) {
            for &k in kids {
                if !out.contains(&k) {
                    out.push(k);
                    stack.push(k);
                }
            }
        }
    }
    out
}

/// 向上找到同一映像名的根进程（Chrome/Electron 场景）。
///
/// 前台窗口进程往往是 renderer 子进程，只有杀到同名的根进程才是"关闭应用"。
pub fn root_of_same_name(pid: u32) -> u32 {
    let entries = snapshot_fresh();
    let by_pid: HashMap<u32, &ProcEntry> = entries.iter().map(|e| (e.pid, e)).collect();
    let Some(start) = by_pid.get(&pid) else {
        return pid;
    };
    let target_name = start.exe_name.to_lowercase();
    let mut cur = pid;
    let mut guard = 0;
    while guard < 32 {
        guard += 1;
        let Some(entry) = by_pid.get(&cur) else { break };
        let Some(parent) = by_pid.get(&entry.ppid) else {
            break;
        };
        if parent.exe_name.to_lowercase() != target_name || parent.pid == cur {
            break;
        }
        cur = parent.pid;
    }
    cur
}
