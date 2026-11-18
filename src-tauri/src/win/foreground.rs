//! 前台窗口 → 进程识别。
//!
//! 链路：GetForegroundWindow → GetAncestor(GA_ROOT) → GetWindowThreadProcessId
//! → OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION) → QueryFullProcessImageNameW。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowTextW,
    GetWindowThreadProcessId, GA_ROOT,
};

use crate::config::normalize_exe_key;

/// 进程映像名查询结果的三态。
#[derive(Debug, Clone)]
pub enum ImageLookup {
    /// 拿到完整路径
    Full(String),
    /// 只拿到文件名（OpenProcess 被拒，降级走 Toolhelp）
    NameOnly(String),
    /// 完全拿不到（PPL 保护进程 / 其他会话）
    Denied,
}

#[derive(Debug, Clone)]
pub struct ForegroundInfo {
    pub pid: u32,
    /// 原始大小写文件名，如 `chrome.exe`
    pub exe_name: String,
    /// 规范化小写完整路径；识别失败为 `pid:<pid>`
    pub exe_key: String,
    pub path: Option<String>,
    pub title: String,
    /// ok | unknown | denied
    pub integrity: String,
}

impl ForegroundInfo {
    pub fn has_path(&self) -> bool {
        self.path.is_some()
    }
}

/// 采样当前前台窗口。切换中 / 安全桌面 / 无前台窗口时返回 None。
pub fn sample_foreground() -> Option<ForegroundInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let target = if root.0.is_null() { hwnd } else { root };

        let mut pid = 0u32;
        GetWindowThreadProcessId(target, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let title = window_title(target);
        let mut info = build_info(target, pid, title);

        // UWP / Store 应用：前台窗口属于 ApplicationFrameHost，真实进程在子窗口里。
        if info.exe_name.eq_ignore_ascii_case("applicationframehost.exe") {
            if let Some(real_pid) = hosted_pid(target) {
                info = build_info(target, real_pid, info.title);
            }
        }

        Some(info)
    }
}

fn build_info(_hwnd: HWND, pid: u32, title: String) -> ForegroundInfo {
    match resolve_image(pid) {
        ImageLookup::Full(path) => {
            let exe_name = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            ForegroundInfo {
                pid,
                exe_name,
                exe_key: normalize_exe_key(&path),
                path: Some(path),
                title,
                integrity: "ok".into(),
            }
        }
        ImageLookup::NameOnly(name) => ForegroundInfo {
            pid,
            exe_key: normalize_exe_key(&name),
            exe_name: name,
            path: None,
            title,
            integrity: "unknown".into(),
        },
        ImageLookup::Denied => ForegroundInfo {
            pid,
            exe_name: format!("pid:{pid}"),
            exe_key: format!("pid:{pid}"),
            path: None,
            title,
            integrity: "denied".into(),
        },
    }
}

/// 进程创建时间（FILETIME 拼成 u64），用于识别 PID 复用。读不到（PPL / 其他会话）返回 None。
pub fn creation_time(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let r = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        let _ = CloseHandle(handle);
        if r.is_err() {
            return None;
        }
        Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }
}

/// `pid → 映像` 缓存，key 必须带创建时间：否则 PID 复用后会把新进程认成旧路径。
const IMAGE_CACHE_CAP: usize = 256;

struct ImageCacheEntry {
    created: u64,
    lookup: ImageLookup,
}

fn image_cache() -> &'static Mutex<HashMap<u32, ImageCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, ImageCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_get(pid: u32, created: u64) -> Option<ImageLookup> {
    let cache = image_cache().lock().ok()?;
    cache
        .get(&pid)
        .filter(|e| e.created == created)
        .map(|e| e.lookup.clone())
}

fn cache_put(pid: u32, created: u64, lookup: &ImageLookup) {
    if let Ok(mut cache) = image_cache().lock() {
        if cache.len() >= IMAGE_CACHE_CAP {
            // 简单粗暴但足够：超限整体清空，避免长跑无限增长
            cache.clear();
        }
        cache.insert(
            pid,
            ImageCacheEntry {
                created,
                lookup: lookup.clone(),
            },
        );
    }
}

/// 查询进程映像：先试完整路径，失败降级到 Toolhelp 文件名。
pub fn resolve_image(pid: u32) -> ImageLookup {
    let created = creation_time(pid);
    if let Some(created) = created {
        if let Some(hit) = cache_get(pid, created) {
            return hit;
        }
    }
    let lookup = query_image(pid);
    if let Some(created) = created {
        cache_put(pid, created, &lookup);
    }
    lookup
}

fn query_image(pid: u32) -> ImageLookup {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                let r = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buf.as_mut_ptr()),
                    &mut len,
                );
                let _ = CloseHandle(handle);
                if r.is_ok() && len > 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    return ImageLookup::Full(path);
                }
                // 句柄拿到了但查不到路径（PPL 等）→ 用 Toolhelp 兜底
                match crate::win::procs::find_name(pid) {
                    Some(n) if !n.is_empty() => ImageLookup::NameOnly(n),
                    _ => ImageLookup::Denied,
                }
            }
            Err(_) => match crate::win::procs::find_name(pid) {
                Some(n) if !n.is_empty() => ImageLookup::NameOnly(n),
                _ => ImageLookup::Denied,
            },
        }
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

struct ChildSearch {
    found: u32,
    depth: u32,
}

/// 从框宿主的顶层窗口反查 UWP 真实进程。
pub fn hosted_pid(parent: HWND) -> Option<u32> {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut ChildSearch);
        ctx.depth += 1;
        if ctx.depth > 4000 {
            return BOOL(0);
        }
        if window_class(hwnd).eq_ignore_ascii_case("Windows.UI.Core.CoreWindow") {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 {
                ctx.found = pid;
                return BOOL(0); // 找到了，停止枚举
            }
        }
        BOOL(1)
    }

    let mut ctx = ChildSearch {
        found: 0,
        depth: 0,
    };
    unsafe {
        let _ = EnumChildWindows(
            Some(parent),
            Some(cb),
            LPARAM(&mut ctx as *mut ChildSearch as isize),
        );
    }
    (ctx.found != 0).then_some(ctx.found)
}

/// 把任意窗口句柄解析成进程信息（屏幕拾取窗口用）。
///
/// 与前台采样同一套归位逻辑：先升到根窗口，再识别进程；遇到 UWP 框宿主时
/// 反查真实应用进程，避免把 `ApplicationFrameHost.exe` 当成目标。
pub fn info_for_hwnd(hwnd: HWND) -> Option<ForegroundInfo> {
    if hwnd.0.is_null() {
        return None;
    }
    unsafe {
        let root = GetAncestor(hwnd, GA_ROOT);
        let target = if root.0.is_null() { hwnd } else { root };

        let mut pid = 0u32;
        GetWindowThreadProcessId(target, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let title = window_title(target);
        let mut info = build_info(target, pid, title);
        if info.exe_name.eq_ignore_ascii_case("applicationframehost.exe") {
            if let Some(real_pid) = hosted_pid(target) {
                info = build_info(target, real_pid, info.title);
            }
        }
        Some(info)
    }
}

/// 某个框宿主进程背后的 UWP 真实 pid。反查不到返回 None——调用方必须放弃处置，
/// 绝不能退化成杀 `ApplicationFrameHost.exe`。
pub fn hosted_pid_of_process(pid: u32) -> Option<u32> {
    for hwnd in crate::win::close::top_level_windows_of(pid) {
        if let Some(real) = hosted_pid(hwnd) {
            if real != pid {
                return Some(real);
            }
        }
    }
    None
}


