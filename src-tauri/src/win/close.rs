//! 关闭前台：向目标进程的可见顶层窗口投递 WM_CLOSE。
//!
//! 用 PostMessage 而不是 SendMessage —— 目标 UI 线程卡死时 SendMessage 会把
//! 我们自己也拖死。

use std::time::{Duration, Instant};

use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, PostMessageW, GWL_EXSTYLE, GW_OWNER, WM_CLOSE, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// 进程已退出
    Exited,
    /// 消息已投递但进程仍在（托盘化程序常见）
    StillRunning,
    /// 目标本来就没了
    NotFound,
    /// 权限不足，无法打开进程
    AccessDenied,
}

struct EnumCtx {
    pid: u32,
    hwnds: Vec<HWND>,
}

/// 请求关闭前台，最多等 `timeout`。
pub fn request_graceful_close(pid: u32, timeout: Duration) -> CloseOutcome {
    // 先拿同步句柄，避免进程在我们投递消息后立刻退出导致句柄打不开
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) };
    let handle = match handle {
        Ok(h) => h,
        Err(_) => {
            return if crate::win::procs::exists(pid) {
                CloseOutcome::AccessDenied
            } else {
                CloseOutcome::NotFound
            };
        }
    };

    let targets = top_level_windows_of(pid);
    if targets.is_empty() {
        // 没有可见窗口：托盘化程序，WM_CLOSE 无从投递
        unsafe {
            let _ = CloseHandle(handle);
        }
        return CloseOutcome::StillRunning;
    }

    for hwnd in &targets {
        unsafe {
            if let Err(e) = PostMessageW(
                Some(*hwnd),
                WM_CLOSE,
                Default::default(),
                Default::default(),
            ) {
                log::warn!("PostMessage(WM_CLOSE) 失败 pid={pid}: {e}");
            }
        }
    }

    let deadline = Instant::now() + timeout;
    let mut exited = false;
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        let wait_ms = remain.as_millis().min(200) as u32;
        let ret = unsafe { WaitForSingleObject(handle, wait_ms.max(1)) };
        if ret == WAIT_OBJECT_0 {
            exited = true;
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    unsafe {
        let _ = CloseHandle(handle);
    }

    if exited {
        CloseOutcome::Exited
    } else {
        CloseOutcome::StillRunning
    }
}

/// 枚举某 pid 的可见、无 owner、非工具窗口的顶层窗口。
pub fn top_level_windows_of(pid: u32) -> Vec<HWND> {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumCtx);
        let mut wnd_pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut wnd_pid));
        if wnd_pid != ctx.pid {
            return BOOL(1);
        }
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        // 有 owner 的是对话框/弹出窗，跳过；工具窗口也跳过
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.0.is_null() {
                return BOOL(1);
            }
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex as u32) & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        ctx.hwnds.push(hwnd);
        BOOL(1)
    }

    let mut ctx = EnumCtx {
        pid,
        hwnds: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut ctx as *mut EnumCtx as isize));
    }
    // 优先返回带标题的窗口：主模块通常是有标题的那个，无标题的多是浮层
    ctx.hwnds
        .sort_by_key(|h| if window_title(*h).is_empty() { 1 } else { 0 });
    ctx.hwnds
}

/// 一次性枚举所有可见顶层窗口，得到 `pid → 主窗标题`。
///
/// 进程选择器要一次拿到几十上百个进程的标题，逐进程调用会重复枚举整棵窗口树。
pub fn window_titles() -> std::collections::HashMap<u32, String> {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        type TitleMap = std::collections::HashMap<u32, String>;
        let map = &mut *(lparam.0 as *mut TitleMap);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.0.is_null() {
                return BOOL(1);
            }
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex as u32) & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }
        let title = window_title(hwnd);
        if title.is_empty() {
            return BOOL(1);
        }
        // EnumWindows 按 Z 序，第一个通常是应用主窗
        map.entry(pid).or_insert(title);
        BOOL(1)
    }

    let mut map: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    unsafe {
        let _ = EnumWindows(
            Some(cb),
            LPARAM(&mut map as *mut std::collections::HashMap<u32, String> as isize),
        );
    }
    map
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}
