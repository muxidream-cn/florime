mod app_state;
mod commands;
mod config;
mod db;
mod error;
mod events;
mod monitor;
mod ui;
mod win;

use std::sync::Arc;

use tauri::Manager;

use app_state::AppState;
use config::model::RuleBook;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 提权重启出来的实例会和「还没退出的旧实例」短暂共存。此时若照常走单实例逻辑，
    // 新实例会被当成二次启动、把请求转给旧实例后自行退出，而旧实例紧接着也退出——
    // 结果应用直接消失。所以带提权参数启动时跳过这把门。
    let elevated_relaunch = std::env::args().any(|a| a == win::ELEVATED_FLAG);
    let mut builder = tauri::Builder::default();
    if !elevated_relaunch {
        // 单实例必须第一个注册
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            ui::windows::show_main(app);
        }));
    } else {
        log::info!("提权重启的新实例：跳过单实例检查");
    }
    let builder = builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            let settings = config::load_settings(&handle);

            // 「默认以管理员启动」：非管理员时请求一次 UAC 提权重启。
            // 用户拒绝就按普通权限继续本次运行，避免反复弹窗。
            //
            // 开发模式必须跳过：`tauri dev` 前面挂着 Vite / 文件监视，这里 exit(0)
            // 之后开发会话接不上，提权实例往往起不完整（表现为弹完 UAC 程序就没了）。
            if settings.launch_as_admin && !win::is_elevated() {
                if cfg!(debug_assertions) {
                    log::info!("开发模式：忽略「默认以管理员启动」");
                } else {
                    log::info!("已启用「默认以管理员启动」，请求提权重启");
                    match win::relaunch_elevated() {
                        Ok(()) => {
                            handle.exit(0);
                            return Ok(());
                        }
                        Err(e) => {
                            log::warn!("提权启动被拒绝或失败，本次以普通权限运行: {e}");
                        }
                    }
                }
            }

            let db_file = config::data_dir(&handle)?.join("focus.db");
            let database = Arc::new(db::Db::open(&db_file)?);
            let rules = config::load_rules(&handle);
            let guard = win::protected::collect_self_guard();

            log::info!(
                "启动: db={}, 规则 {} 条, 自保护 pid {:?}",
                db_file.display(),
                rules.rules.len(),
                guard.pids
            );

            let state = Arc::new(AppState::new(
                database.clone(),
                settings.clone(),
                RuleBook {
                    rules: rules.rules.clone(),
                },
                guard,
            ));
            app.manage(state.clone());

            ui::tray::build(&handle)?;

            let monitor = monitor::spawn(handle.clone(), state.clone());
            *state.monitor.write().expect("monitor poisoned") = Some(monitor);

            // 首次启动把默认配置落盘，方便用户直接改 JSON
            if !config::settings_path(&handle)?.exists() {
                let _ = config::save_settings(&handle, &settings);
            }
            if !config::rules_path(&handle)?.exists() {
                let _ = config::save_rules(&handle, &rules);
            }

            // 过期数据清理丢到后台，别拖慢启动
            let db_prune = database.clone();
            let retention = settings.retention_days;
            std::thread::spawn(move || {
                match db_prune.prune(retention) {
                    Ok(n) if n > 0 => log::info!("清理过期明细 {n} 行"),
                    Ok(_) => {}
                    Err(e) => log::warn!("清理过期数据失败: {e}"),
                }
            });

            // 等前端首帧渲染完再亮窗，避免白屏闪烁
            let show_handle = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(280));
                if let Some(w) = show_handle.get_webview_window(ui::windows::MAIN_LABEL) {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != ui::windows::MAIN_LABEL {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 拾取钩子会吞掉鼠标点击：主窗一关（或收进托盘）就必须收掉，
                // 否则用户会发现点什么都没反应，最长要等 15 秒超时。
                win::picker::stop();
                let app = window.app_handle();
                let close_to_tray = app
                    .try_state::<Arc<AppState>>()
                    .map(|s| {
                        s.settings_snapshot().close_behavior
                            == config::model::CloseBehavior::MinimizeToTray
                    })
                    .unwrap_or(true);

                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    if let Some(s) = app.try_state::<Arc<AppState>>() {
                        s.notify_monitor(monitor::MonitorCmd::Shutdown);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_snapshot,
            commands::pause_monitor,
            commands::snooze_rule,
            commands::enforce_now,
            commands::force_kill_tree,
            commands::get_active_alert,
            commands::get_rules,
            commands::upsert_rule,
            commands::delete_rule,
            commands::set_rule_enabled,
            commands::set_rule_paused,
            commands::reorder_rules,
            commands::get_settings,
            commands::update_settings,
            commands::is_autostart_enabled,
            commands::list_running_processes,
            commands::get_app_meta,
            commands::query_day_stats,
            commands::query_day_detail,
            commands::query_top_exes,
            commands::query_grand_total,
            commands::query_day_category_stats,
            commands::export_csv,
            commands::open_data_dir,
            commands::reveal_rule_path,
            commands::db_path,
            commands::is_elevated,
            commands::restart_as_admin,
            commands::dismiss_alert,
            commands::rest_ready,
            commands::hide_rest,
            commands::start_window_pick,
            commands::cancel_window_pick,
        ]);

    let app = match builder.build(tauri::generate_context!()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("启动失败: {e}");
            return;
        }
    };

    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            // 低级钩子必须显式卸载：异常退出时别把用户的鼠标继续吞着
            win::picker::stop();
            win::shield::stop();
            // 保证退出前把内存里的计时尾巴写完
            if let Some(state) = handle.try_state::<Arc<AppState>>() {
                // 取走 handle 并 shutdown：阻塞等监控线程把尾巴写完
                let taken = state
                    .monitor
                    .write()
                    .ok()
                    .and_then(|mut guard| guard.take());
                if let Some(h) = taken {
                    h.shutdown();
                }
            }
        }
    });
}
