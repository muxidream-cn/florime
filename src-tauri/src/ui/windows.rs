//! 窗口管理：主窗、超限提醒窗与休息提醒窗。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::events::{AlertPayload, RestPayload};

pub const MAIN_LABEL: &str = "main";
pub const TOAST_LABEL: &str = "toast";
/// 专注模式的休息提醒：独立窗口，避免和超限告警互相覆盖。
pub const REST_LABEL: &str = "rest";

/// 提醒窗逻辑尺寸（CSS px）。创建窗口与定位共用同一组常量，
/// 避免改了一处忘了另一处导致贴边错位。
const TOAST_WIDTH: f64 = 400.0;
const TOAST_HEIGHT: f64 = 220.0;

/// 提醒窗前端是否已完成启动探针。冷启动的 WebView2 会先黑一帧，
/// 所以第一次弹窗要等页面就绪再 show，否则用户看到的是黑方块。
static TOAST_READY: AtomicBool = AtomicBool::new(false);
/// 等待显示的告警：页面就绪后补发一次并显示。
static PENDING_ALERT: Mutex<Option<AlertPayload>> = Mutex::new(None);
/// 休息提醒窗的就绪探针与挂起载荷。
static REST_READY: AtomicBool = AtomicBool::new(false);
static PENDING_REST: Mutex<Option<RestPayload>> = Mutex::new(None);
/// 两个提醒窗当前是否处于全屏形态。只在形态变化时切换窗口属性，
/// 否则清理阶段每轮重弹都会「退出全屏再进入」，肉眼就是一秒闪白。
static TOAST_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static REST_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 创建（或复用）一个无边框、置顶、跳过任务栏的提醒窗。初始不显示。
fn ensure_window(app: &AppHandle, label: &str, route: &str, title: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }

    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(route.into()))
        .title(title)
        .inner_size(TOAST_WIDTH, TOAST_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false);

    match builder.build() {
        Ok(w) => Some(w),
        Err(e) => {
            log::error!("创建窗口 {label} 失败: {e}");
            None
        }
    }
}

fn ensure_toast(app: &AppHandle) -> Option<WebviewWindow> {
    ensure_window(app, TOAST_LABEL, "index.html#/toast", "莳间提醒")
}

fn ensure_rest(app: &AppHandle) -> Option<WebviewWindow> {
    ensure_window(app, REST_LABEL, "index.html#/rest", "莳间 · 休息提醒")
}

/// 前台窗口所在显示器 → 光标所在显示器 → 主显示器。
fn target_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowRect,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.0.is_null() {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let cx = (rect.left + rect.right) as f64 / 2.0;
                let cy = (rect.top + rect.bottom) as f64 / 2.0;
                if let Ok(Some(m)) = app.monitor_from_point(cx, cy) {
                    return Some(m);
                }
            }
        }

        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            if let Ok(Some(m)) = app.monitor_from_point(pt.x as f64, pt.y as f64) {
                return Some(m);
            }
        }
    }

    app.primary_monitor().ok().flatten()
}

/// 贴着当前显示器右下角摆放。每次弹出前重算，用户拖走过也能复位。
fn position_toast(app: &AppHandle, w: &WebviewWindow) {
    let Some(monitor) = target_monitor(app) else {
        return;
    };
    let size = monitor.size();
    let pos = monitor.position();
    let scale = monitor.scale_factor();
    let ww = (TOAST_WIDTH * scale) as i32;
    let wh = (TOAST_HEIGHT * scale) as i32;
    let margin = (24.0 * scale) as i32;
    let x = pos.x + size.width as i32 - ww - margin;
    let y = pos.y + size.height as i32 - wh - margin * 3;
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// 应用提醒窗形态：全屏 = 覆盖当前显示器 + 置顶 + 抢焦点；否则贴边小窗。
///
/// `current` 记录该窗上一次的形态，只有真的变了才重设属性：
/// 全屏下反复 `set_fullscreen(false)` 会让窗口闪一下。
fn apply_alert_shape(app: &AppHandle, w: &WebviewWindow, fullscreen: bool, current: &AtomicBool) {
    if current.load(Ordering::Relaxed) == fullscreen {
        if !fullscreen {
            // 小窗每次弹出都重新贴边，用户拖走过也能复位
            position_toast(app, w);
        }
        let _ = w.show();
        return;
    }
    current.store(fullscreen, Ordering::Relaxed);

    if fullscreen {
        // 先挪到目标显示器再全屏，保证盖住的是「用户正在被监控的那块屏」
        let _ = w.set_fullscreen(false);
        let _ = w.set_size(LogicalSize::new(TOAST_WIDTH, TOAST_HEIGHT));
        position_toast(app, w);
        let _ = w.set_always_on_top(true);
        let _ = w.set_fullscreen(true);
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }

    let _ = w.set_fullscreen(false);
    let _ = w.set_size(LogicalSize::new(TOAST_WIDTH, TOAST_HEIGHT));
    position_toast(app, w);
    let _ = w.show();
}

/// 只隐藏提醒窗，并卸载键盘屏蔽。
///
/// 收窗时不能退出全屏或改尺寸：Windows/WebView2 对隐藏中的全屏窗做形态复位，
/// 可能让它重新以 400×220 小窗显形；内容此时已清空，用户看到的就是右下角黑块。
/// 形态复位统一交给下一次显示前的 `apply_alert_shape()`。
fn hide_alert(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.hide();
    }
    crate::win::shield::stop();
}

/// 定位 + 推数据 + 按形态显示。此时页面一定已经渲染过。
fn reveal<T: Serialize + Clone>(
    app: &AppHandle,
    label: &'static str,
    event: &'static str,
    payload: &T,
    fullscreen: bool,
    shape: &'static AtomicBool,
) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    // 先推数据再显示，避免窗口亮起来还是上一轮的旧内容
    let _ = app.emit_to(label, event, payload.clone());
    apply_alert_shape(app, &w, fullscreen, shape);
}

/// 页面就绪后补弹挂起的窗口；首次弹窗等待探针，最多等 2 秒。
fn reveal_after_ready<T: Serialize + Clone + Send + 'static>(
    app: &AppHandle,
    label: &'static str,
    event: &'static str,
    ready: &'static AtomicBool,
    pending: &'static Mutex<Option<T>>,
    shape: &'static AtomicBool,
    fullscreen: bool,
) {
    if ready.load(Ordering::Relaxed) {
        if let Some(payload) = pending.lock().ok().and_then(|mut g| g.take()) {
            reveal(app, label, event, &payload, fullscreen, shape);
        }
        return;
    }

    log::info!("窗口 {label} 首次创建，等待前端探针");
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2000));
        if ready.load(Ordering::Relaxed) {
            return;
        }
        let payload = pending.lock().ok().and_then(|mut g| g.take());
        if let Some(payload) = payload {
            // 宁可看到一帧黑，也不能漏掉提醒
            log::warn!("窗口 {label} 2 秒内未就绪，强制显示");
            reveal(&app, label, event, &payload, fullscreen, shape);
        }
    });
}

/// 提醒窗页面就绪（由前端 `ping` 触发）。有挂起告警就立刻补弹。
pub fn mark_toast_ready(app: &AppHandle) {
    TOAST_READY.store(true, Ordering::Relaxed);
    let pending = PENDING_ALERT.lock().ok().and_then(|mut guard| guard.take());
    if let Some(alert) = pending {
        let fullscreen = alert.fullscreen;
        reveal(
            app,
            TOAST_LABEL,
            crate::events::EV_ALERT,
            &alert,
            fullscreen,
            &TOAST_FULLSCREEN,
        );
    }
}

/// 休息提醒窗页面就绪（由前端 `rest_ready` 触发）。
pub fn mark_rest_ready(app: &AppHandle) {
    REST_READY.store(true, Ordering::Relaxed);
    let pending = PENDING_REST.lock().ok().and_then(|mut guard| guard.take());
    if let Some(rest) = pending {
        let fullscreen = rest.fullscreen;
        reveal(
            app,
            REST_LABEL,
            crate::events::EV_REST,
            &rest,
            fullscreen,
            &REST_FULLSCREEN,
        );
    }
}

/// 弹出提醒窗并刷新内容。
pub fn show_toast(app: &AppHandle, alert: &AlertPayload) {
    if ensure_toast(app).is_none() {
        return;
    }
    if let Ok(mut guard) = PENDING_ALERT.lock() {
        *guard = Some(alert.clone());
    }
    reveal_after_ready(
        app,
        TOAST_LABEL,
        crate::events::EV_ALERT,
        &TOAST_READY,
        &PENDING_ALERT,
        &TOAST_FULLSCREEN,
        alert.fullscreen,
    );
}

pub fn hide_toast(app: &AppHandle) {
    if let Ok(mut guard) = PENDING_ALERT.lock() {
        *guard = None;
    }
    hide_alert(app, TOAST_LABEL);
}

/// 弹出休息提醒。形态（小窗 / 全屏）由规则配置决定。
pub fn show_rest(app: &AppHandle, payload: &RestPayload) {
    if ensure_rest(app).is_none() {
        return;
    }
    if let Ok(mut guard) = PENDING_REST.lock() {
        *guard = Some(payload.clone());
    }
    reveal_after_ready(
        app,
        REST_LABEL,
        crate::events::EV_REST,
        &REST_READY,
        &PENDING_REST,
        &REST_FULLSCREEN,
        payload.fullscreen,
    );
}

/// 休息持续时间到点后重新显示窗口，只发确认事件，不重启倒计时。
pub fn show_rest_confirm(app: &AppHandle, payload: &RestPayload) {
    if ensure_rest(app).is_none() {
        return;
    }
    reveal(
        app,
        REST_LABEL,
        crate::events::EV_REST_CONFIRM,
        payload,
        payload.fullscreen,
        &REST_FULLSCREEN,
    );
}

pub fn hide_rest(app: &AppHandle) {
    if let Ok(mut guard) = PENDING_REST.lock() {
        *guard = None;
    }
    hide_alert(app, REST_LABEL);
}
