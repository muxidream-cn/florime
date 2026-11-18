//! 系统关键进程保护表。绝不参与计时，绝不参与关闭/强杀。

/// 永远不碰的进程名（小写，含 .exe）。
const SYSTEM_CRITICAL: &[&str] = &[
    "system",
    "system idle process",
    "registry",
    "memory compression",
    "secure system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "explorer.exe",
    "sihost.exe",
    "taskhostw.exe",
    "ctfmon.exe",
    "runtimebroker.exe",
    "searchhost.exe",
    "searchindexer.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "applicationframehost.exe",
    "audiodg.exe",
    "spoolsv.exe",
    "consent.exe",
    "logonui.exe",
    "userinit.exe",
    "wmiprvse.exe",
    "wininit.exe",
    "msmpeng.exe",
    "nissrv.exe",
    "securityhealthservice.exe",
    "securityhealthsystray.exe",
    "msedgewebview2.exe",
];

/// 自身不应被杀（由调用方传入，运行时采集）。
#[derive(Debug, Default, Clone)]
pub struct SelfGuard {
    /// 自身 + 祖先链 + 全部 WebView2 子进程
    pub pids: Vec<u32>,
}

impl SelfGuard {
    pub fn contains(&self, pid: u32) -> bool {
        self.pids.contains(&pid)
    }
}

/// 处置前的会话校验。
///
/// `Some(true)` 目标与应用同会话；`Some(false)` 在别的会话（服务 / 其他登录用户）；
/// `None` 表示 API 读不到（PPL 保护进程等）。
/// 调用约定：销毁性动作在 `None` 时按拒绝处理（fail-closed）；仅识别归属时放行（fail-open）。
pub fn session_allows(pid: u32) -> Option<bool> {
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;

    let mut target = 0u32;
    if unsafe { ProcessIdToSessionId(pid, &mut target) }.is_err() {
        return None;
    }
    Some(target == self_session())
}

/// 自身所在会话号，只取一次。
fn self_session() -> u32 {
    use std::sync::OnceLock;
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;

    static SELF_SESSION: OnceLock<u32> = OnceLock::new();
    *SELF_SESSION.get_or_init(|| {
        let mut sid = 0u32;
        unsafe {
            let _ = ProcessIdToSessionId(std::process::id(), &mut sid);
        }
        sid
    })
}

/// 是否位于 Windows 目录下（系统自带程序）。
///
/// 与「受保护」分开判定：进程选择器里「忽略系统项」是纯展示过滤，
/// 而受保护进程即使展示出来也绝不允许被配成处置目标。
pub fn is_system_path(exe_key: &str) -> bool {
    let Ok(windir) = std::env::var("SystemRoot") else {
        return false;
    };
    let prefix = crate::config::normalize_exe_key(&windir);
    exe_key
        .to_lowercase()
        .starts_with(&format!("{prefix}\\"))
}

/// 判定是否受保护。
///
/// `exe_key` 是规范化小写完整路径（可能是 `pid:1234`），`exe_name` 是原始文件名。
pub fn is_protected(
    exe_key: &str,
    exe_name: &str,
    pid: u32,
    guard: &SelfGuard,
    extra: &[String],
) -> bool {
    if guard.contains(pid) {
        return true;
    }
    // pid 0/4 是 System，绝对禁止
    if pid == 0 || pid == 4 {
        return true;
    }

    // 会话闸：只处理自己所在会话里的进程
    if session_allows(pid) == Some(false) {
        return true;
    }

    let name = exe_name.to_lowercase();
    if SYSTEM_CRITICAL.iter().any(|n| *n == name) {
        return true;
    }

    // Windows 目录下的一切都不碰
    if is_system_path(exe_key) {
        return true;
    }

    // 用户自定义保护项（可以是完整路径或文件名）
    let lower_key = exe_key.to_lowercase();
    extra
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&lower_key) || p.eq_ignore_ascii_case(&name))
}

/// 采集自身保护集：自身 pid、祖先链、以及所有 WebView2 渲染进程。
pub fn collect_self_guard() -> SelfGuard {
    use crate::win::procs;

    let me = std::process::id();
    let entries = procs::snapshot_fresh();
    let by_pid: std::collections::HashMap<u32, &procs::ProcEntry> =
        entries.iter().map(|e| (e.pid, e)).collect();

    let mut pids = vec![me];
    // 祖先链：杀掉父进程会连带影响自身
    let mut cur = me;
    for _ in 0..16 {
        let Some(entry) = by_pid.get(&cur) else { break };
        if entry.ppid == 0 || entry.ppid == cur || pids.contains(&entry.ppid) {
            break;
        }
        pids.push(entry.ppid);
        cur = entry.ppid;
    }

    // WebView2 渲染进程：名字是 msedgewebview2.exe 且父链经过自身
    for e in &entries {
        if !e.exe_name.eq_ignore_ascii_case("msedgewebview2.exe") {
            continue;
        }
        let mut c = e.pid;
        for _ in 0..16 {
            let Some(entry) = by_pid.get(&c) else { break };
            if entry.ppid == me || pids.contains(&entry.ppid) {
                if !pids.contains(&e.pid) {
                    pids.push(e.pid);
                }
                break;
            }
            if entry.ppid == 0 || entry.ppid == c {
                break;
            }
            c = entry.ppid;
        }
    }

    SelfGuard { pids }
}
