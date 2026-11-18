//! 托盘：常驻入口、暂停开关、今日用量 tooltip。

use std::sync::Arc;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::app_state::AppState;
use crate::monitor::MonitorCmd;

pub struct TrayState {
    pub pause_item: MenuItem<Wry>,
    pub usage_item: MenuItem<Wry>,
}

const ID_OPEN: &str = "tray-open";
const ID_PAUSE: &str = "tray-pause";
const ID_QUIT: &str = "tray-quit";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, ID_OPEN, "打开主界面", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, ID_PAUSE, "暂停专注", true, None::<&str>)?;
    let usage = MenuItem::with_id(app, "tray-usage", "今日 0 分钟", false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "退出莳间", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open, &pause, &usage, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("florime-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("莳间")
        .on_menu_event(|app, event| match event.id().as_ref() {
            ID_OPEN => crate::ui::windows::show_main(app),
            ID_PAUSE => toggle_pause(app),
            ID_QUIT => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::ui::windows::show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    app.manage(TrayState {
        pause_item: pause,
        usage_item: usage,
    });

    spawn_tooltip_updater(app.clone());
    Ok(())
}

fn toggle_pause(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let current = is_paused(app);
    state.notify_monitor(MonitorCmd::Pause(!current));
    // 先本地切文案给即时反馈，监控线程随后会再同步一次真实状态
    if let Some(tray) = app.try_state::<TrayState>() {
        let _ = tray.pause_item.set_text(if current {
            "暂停专注"
        } else {
            "继续专注"
        });
    }
}

/// 由监控线程直接调用：状态一变就刷新菜单文案与 tooltip（兜底轮询仍在跑）。
pub fn sync_state(app: &AppHandle, paused: bool, today_total_ms: u64) {
    let minutes = today_total_ms / 60_000;
    let tip = format!(
        "莳间 · 今日 {minutes} 分钟{}",
        if paused { " · 已暂停" } else { "" }
    );
    if let Some(tray) = app.tray_by_id("florime-tray") {
        let _ = tray.set_tooltip(Some(&tip));
    }
    if let Some(ts) = app.try_state::<TrayState>() {
        let _ = ts.usage_item.set_text(format!("今日 {minutes} 分钟"));
        let _ = ts.pause_item.set_text(if paused {
            "继续专注"
        } else {
            "暂停专注"
        });
    }
}

fn is_paused(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return false;
    };
    let monitor = state.monitor.read().expect("monitor poisoned");
    match monitor.as_ref() {
        Some(h) => h.shared().lock().expect("shared poisoned").paused,
        None => false,
    }
}

fn quit_app(app: &AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.notify_monitor(MonitorCmd::Shutdown);
    }
    // 给监控线程一点时间把尾巴写完
    std::thread::sleep(Duration::from_millis(300));
    app.exit(0);
}

/// 兜底轮询：主链路是监控线程的 `sync_state`，这里只防万一（例如线程被暂停）。
fn spawn_tooltip_updater(app: AppHandle) {
    std::thread::Builder::new()
        .name("florime-tray-tip".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));

            let Some(state) = app.try_state::<Arc<AppState>>() else {
                return;
            };
            let monitor = state.monitor.read().expect("monitor poisoned");
            let Some(h) = monitor.as_ref() else {
                continue;
            };
            let (paused, total_ms) = {
                let sh = h.shared();
                let g = sh.lock().expect("shared poisoned");
                (g.paused, g.snapshot.today_total_ms)
            };
            drop(monitor);

            sync_state(&app, paused, total_ms);
        })
        .ok();
}
