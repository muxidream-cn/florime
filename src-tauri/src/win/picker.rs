//! 屏幕拾取窗口：进入拾取模式后点击任意应用窗口，取到该窗口所属进程。
//!
//! 与 `shield` 同一套线程范式：独立线程装低级钩子、跑消息泵、`AtomicBool` 控生命周期，
//! 看门狗到点强制取消。区别是这里装的是 `WH_MOUSE_LL`（拾取）与一个只看 Esc 的
//! `WH_KEYBOARD_LL`（取消），并且**必须与快捷键屏蔽互斥**：拾取期间不能吞掉 Win/Alt+Tab。
//!
//! 取舍：拾取期间吞掉点击，避免用户点选窗口的同时误触目标应用（例如点中"关闭"按钮）。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, HC_ACTION, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, MSG,
    PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP,
};

use crate::events::PickedWindow;
use crate::win::foreground;
use crate::win::protected::{self, SelfGuard};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// 每次 start/stop 递增；钩子线程与看门狗靠它判断自己是否已过期
static GENERATION: AtomicU64 = AtomicU64::new(0);

static HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
static GUARD: Mutex<Option<SelfGuard>> = Mutex::new(None);
static EXTRA: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// 进入拾取模式。重复调用只刷新看门狗，不会叠加钩子。
pub fn start(app: AppHandle, guard: SelfGuard, extra: Vec<String>, timeout: Duration) {
    // 拾取与快捷键屏蔽互斥：否则用户连 Win/Alt+Tab 都用不了，更别说点窗口
    crate::win::shield::stop();

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut h) = HANDLE.lock() {
        *h = Some(app);
    }
    if let Ok(mut g) = GUARD.lock() {
        *g = Some(guard);
    }
    if let Ok(mut e) = EXTRA.lock() {
        *e = extra;
    }

    if !ACTIVE.swap(true, Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("florime-picker".into())
            .spawn(move || unsafe { run_hook_loop(generation) });
        if spawned.is_err() {
            ACTIVE.store(false, Ordering::SeqCst);
            log::error!("无法启动窗口拾取线程");
            emit_cancelled("spawn_failed");
            return;
        }
    }

    // 看门狗：到点一定取消，绝不让鼠标钩子常驻
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        if GENERATION.load(Ordering::SeqCst) == generation {
            log::warn!("窗口拾取超时，自动取消");
            emit_cancelled("timeout");
        }
    });
}

/// 停止拾取；幂等，任何收敛路径都可以无脑调用。
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let tid = THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    log::info!("窗口拾取已停止");
}

unsafe fn run_hook_loop(generation: u64) {
    let mut msg = MSG::default();
    // 先建消息队列，否则 PostThreadMessageW 可能投递失败
    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);

    let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) {
        Ok(h) => h,
        Err(e) => {
            log::error!("安装鼠标拾取钩子失败: {e}");
            release(generation);
            return;
        }
    };
    let keyboard = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
        Ok(h) => h,
        Err(e) => {
            log::error!("安装拾取取消键钩子失败: {e}");
            let _ = UnhookWindowsHookEx(mouse);
            release(generation);
            return;
        }
    };

    THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);

    // 安装期间可能已被取消：此时没人能给我们发 WM_QUIT，必须自己卸载退出
    if GENERATION.load(Ordering::SeqCst) != generation {
        log::info!("窗口拾取在就绪前已被取消，直接卸载");
        let _ = UnhookWindowsHookEx(mouse);
        let _ = UnhookWindowsHookEx(keyboard);
        THREAD_ID.store(0, Ordering::SeqCst);
        return;
    }

    log::info!("窗口拾取已启用，点击任意应用窗口即可选中");

    loop {
        // WM_QUIT 返回 0，出错返回 -1：两种情况都必须退出，否则会忙等
        if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
            break;
        }
    }

    let _ = UnhookWindowsHookEx(mouse);
    let _ = UnhookWindowsHookEx(keyboard);
    release(generation);
    log::info!("窗口拾取钩子已卸载");
}

/// 只有仍是当前这一代时才复位状态，避免把新一轮 start 的状态清掉。
fn release(generation: u64) {
    if GENERATION.load(Ordering::SeqCst) == generation {
        THREAD_ID.store(0, Ordering::SeqCst);
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 || !ACTIVE.load(Ordering::Relaxed) {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    match wparam.0 as u32 {
        WM_LBUTTONDOWN => {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            match resolve_at(info.pt) {
                Some(picked) => {
                    log::info!("拾取到窗口 pid={} exe={}", picked.pid, picked.exe_name);
                    emit_picked(picked);
                }
                // 点到自己窗口/桌面：不结束拾取，让用户继续选
                None => log::debug!("拾取忽略：目标不可用或属于自身"),
            }
            // 成功与否都吞掉这次点击，避免误触目标应用
            LRESULT(1)
        }
        WM_RBUTTONDOWN => {
            log::info!("右键取消窗口拾取");
            emit_cancelled("right_click");
            LRESULT(1)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => LRESULT(1),
        _ => CallNextHookEx(None, code, wparam, lparam),
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && ACTIVE.load(Ordering::Relaxed) {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if info.vkCode == u32::from(VK_ESCAPE.0) {
            log::info!("Esc 取消窗口拾取");
            emit_cancelled("escape");
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// 把屏幕坐标解析成目标进程；点到自己、桌面或无法识别时返回 None。
unsafe fn resolve_at(pt: POINT) -> Option<PickedWindow> {
    let hwnd = WindowFromPoint(pt);
    if hwnd.0.is_null() {
        return None;
    }

    let info = foreground::info_for_hwnd(hwnd)?;
    if info.pid == 0 {
        return None;
    }

    let guard = GUARD.lock().ok().and_then(|g| g.clone())?;
    if guard.contains(info.pid) {
        return None;
    }
    // 任务栏/桌面都属于 explorer：选中它没有意义，避免误建规则
    if info.exe_name.eq_ignore_ascii_case("explorer.exe") {
        return None;
    }

    let extra = EXTRA.lock().map(|e| e.clone()).unwrap_or_default();
    let is_system = protected::is_system_path(&info.exe_key);
    let is_protected =
        protected::is_protected(&info.exe_key, &info.exe_name, info.pid, &guard, &extra);

    Some(PickedWindow {
        pid: info.pid,
        exe_name: info.exe_name,
        path: info.path,
        title: info.title,
        is_protected,
        is_system,
    })
}

fn emit_picked(payload: PickedWindow) {
    if let Some(app) = HANDLE.lock().ok().and_then(|h| h.clone()) {
        let _ = app.emit(crate::events::EV_WINDOW_PICKED, payload);
    }
    stop();
}

fn emit_cancelled(reason: &str) {
    if let Some(app) = HANDLE.lock().ok().and_then(|h| h.clone()) {
        let _ = app.emit(crate::events::EV_WINDOW_PICK_CANCELLED, reason.to_string());
    }
    stop();
}
