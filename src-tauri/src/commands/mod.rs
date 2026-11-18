use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::config::model::{MatchKind, Rule, RuleInput, RuleBook, RuleKind, Settings, SettingsPatch};
use crate::config::{normalize_pattern, save_rules, save_settings};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::events::{AlertPayload, Snapshot};
use crate::monitor::MonitorCmd;
use crate::win::{procs, protected};

type St<'a> = State<'a, Arc<AppState>>;

/// 能力探针：每个窗口启动时打一次，确认 capabilities 配对。
///
/// 提醒窗打完探针即视为「页面已渲染」——首次弹窗前必须等到这一步，否则会闪黑框。
#[tauri::command]
pub fn ping(window: tauri::WebviewWindow) -> String {
    if window.label() == crate::ui::windows::TOAST_LABEL {
        crate::ui::windows::mark_toast_ready(window.app_handle());
    }
    "pong".into()
}

/// 休息提醒窗的就绪探针：窗口首次创建时补弹挂起的载荷。
#[tauri::command]
pub fn rest_ready(window: tauri::WebviewWindow) {
    if window.label() == crate::ui::windows::REST_LABEL {
        crate::ui::windows::mark_rest_ready(window.app_handle());
    }
}

/// 收起休息提醒并恢复计时（前端倒计时结束 / 用户主动关闭）。
#[tauri::command]
pub fn hide_rest(state: St<'_>) {
    state.notify_monitor(MonitorCmd::HideRest);
}

#[tauri::command]
pub fn get_snapshot(state: St<'_>) -> Snapshot {
    let monitor = state.monitor.read().expect("monitor poisoned");
    match monitor.as_ref() {
        Some(h) => {
            let sh = h.shared();
            let guard = sh.lock().expect("shared poisoned");
            guard.snapshot.clone()
        }
        None => Snapshot::default(),
    }
}

#[tauri::command]
pub fn pause_monitor(state: St<'_>, paused: bool) {
    state.notify_monitor(MonitorCmd::Pause(paused));
}

#[tauri::command]
pub fn snooze_rule(state: St<'_>, rule_id: String, secs: u64) {
    state.notify_monitor(MonitorCmd::Snooze { rule_id, secs });
}

#[tauri::command]
pub fn enforce_now(state: St<'_>, rule_id: String) {
    state.notify_monitor(MonitorCmd::EnforceNow { rule_id });
}

/// 清理弹窗的「强制关闭（结束进程树）」：唯一会展开进程树的入口。
#[tauri::command]
pub fn force_kill_tree(state: St<'_>, rule_id: String) {
    state.notify_monitor(MonitorCmd::ForceKillTree { rule_id });
}

#[tauri::command]
pub fn get_active_alert(state: St<'_>) -> Option<AlertPayload> {
    let monitor = state.monitor.read().expect("monitor poisoned");
    let h = monitor.as_ref()?;
    let sh = h.shared();
    let guard = sh.lock().expect("shared poisoned");
    guard.alerts.values().next().cloned()
}

/// 用户点「我知道了」：立即收起提醒窗。
///
/// 收窗不等于取消任务：清理阶段由监控线程保留状态，下一轮复检会重新弹出，
/// 仪表盘也会持续显示「等待后台清理」，直到按规则匹配的进程全部消失。
#[tauri::command]
pub fn dismiss_alert(app: AppHandle, state: St<'_>, rule_id: String) {
    state.notify_monitor(MonitorCmd::DismissAlert { rule_id });
    crate::ui::windows::hide_toast(&app);
}

// ---------------- 规则 ----------------

#[tauri::command]
pub fn get_rules(state: St<'_>) -> Vec<Rule> {
    state.rules_snapshot().rules
}

fn validate(input: &RuleInput) -> AppResult<()> {
    if input.display_name.trim().is_empty() {
        return Err(AppError::msg("名称不能为空"));
    }
    if input.pattern.trim().is_empty() {
        return Err(AppError::msg("匹配值不能为空"));
    }
    // 完整路径却只填了进程名 → 规则永远匹配不上，必须在保存前拦下
    if input.match_kind == MatchKind::FullPath
        && !input.pattern.contains('\\')
        && !input.pattern.contains('/')
    {
        return Err(AppError::msg(
            "完整路径需要包含目录，例如 C:\\App\\x.exe；只想按进程名匹配请改用「仅进程名」",
        ));
    }
    if input.daily_limit_secs == 0 {
        return Err(AppError::msg("每日上限必须大于 0"));
    }
    if input.grace_secs > input.daily_limit_secs {
        return Err(AppError::msg("宽限期不能超过每日上限"));
    }
    Ok(())
}

#[tauri::command]
pub fn upsert_rule(app: AppHandle, state: St<'_>, input: RuleInput) -> AppResult<Vec<Rule>> {
    validate(&input)?;

    let mut book = state.rules_snapshot();
    let id = if input.id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.id.clone()
    };
    let mut rule = Rule {
        id: id.clone(),
        display_name: input.display_name.trim().to_string(),
        match_kind: input.match_kind,
        pattern: normalize_pattern(input.match_kind, &input.pattern),
        mode: input.mode,
        daily_limit_secs: input.daily_limit_secs,
        grace_secs: input.grace_secs,
        on_exceed: input.on_exceed,
        allow_background: input.allow_background,
        enabled: input.enabled,
        paused: input.paused,
        kind: input.kind,
        rest_remind_secs: input.rest_remind_secs,
        rest_auto_continue: input.rest_auto_continue,
        rest: input.rest,
        exceed_remind: input.exceed_remind,
        exceed: input.exceed,
    };
    // 收敛互相矛盾的开关键与越界数值，避免脏配置进监控线程
    rule.normalize();

    if let Some(existing) = book.rules.iter_mut().find(|r| r.id == id) {
        *existing = rule;
    } else {
        book.rules.push(rule);
    }
    // 不再按名称排序：顺序由用户在界面上拖拽决定，编辑/新建都不该打乱它

    persist_rules(&app, state.inner(), &book)?;
    Ok(book.rules)
}

/// 按 `ids` 重排规则：未出现在 ids 里的规则按原顺序补到末尾，重复 id 只取第一次。
///
/// 前端只传当前看到的顺序（可能被过滤过），所以这里必须兜底，绝不能因为漏传就丢规则。
fn apply_rule_order(rules: &mut Vec<Rule>, ids: &[String]) {
    let mut slots: Vec<Option<Rule>> = rules.drain(..).map(Some).collect();
    let mut index: HashMap<String, usize> = HashMap::with_capacity(slots.len());
    for (i, slot) in slots.iter().enumerate() {
        if let Some(rule) = slot {
            index.insert(rule.id.clone(), i);
        }
    }

    let mut ordered: Vec<Rule> = Vec::with_capacity(slots.len());
    for id in ids {
        // take 之后位置变成 None：重复 id 的第二次自然取不到
        if let Some(pos) = index.get(id).copied() {
            if let Some(rule) = slots[pos].take() {
                ordered.push(rule);
            }
        }
    }
    for rule in slots.into_iter().flatten() {
        ordered.push(rule);
    }
    *rules = ordered;
}

/// 保存界面上的拖拽顺序。
#[tauri::command]
pub fn reorder_rules(app: AppHandle, state: St<'_>, ids: Vec<String>) -> AppResult<Vec<Rule>> {
    let mut book = state.rules_snapshot();
    apply_rule_order(&mut book.rules, &ids);
    persist_rules(&app, state.inner(), &book)?;
    Ok(book.rules)
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, state: St<'_>, rule_id: String) -> AppResult<Vec<Rule>> {
    let mut book = state.rules_snapshot();
    book.rules.retain(|r| r.id != rule_id);
    persist_rules(&app, state.inner(), &book)?;
    Ok(book.rules)
}

#[tauri::command]
pub fn set_rule_enabled(
    app: AppHandle,
    state: St<'_>,
    rule_id: String,
    enabled: bool,
) -> AppResult<Vec<Rule>> {
    let mut book = state.rules_snapshot();
    if let Some(r) = book.rules.iter_mut().find(|r| r.id == rule_id) {
        r.enabled = enabled;
    }
    persist_rules(&app, state.inner(), &book)?;
    Ok(book.rules)
}

/// 暂停/恢复单条规则：暂停时不计时、不推进额度、不触发休息；配置仍保留。
#[tauri::command]
pub fn set_rule_paused(
    app: AppHandle,
    state: St<'_>,
    rule_id: String,
    paused: bool,
) -> AppResult<Vec<Rule>> {
    let mut book = state.rules_snapshot();
    if let Some(r) = book.rules.iter_mut().find(|r| r.id == rule_id) {
        r.paused = paused;
    }
    persist_rules(&app, state.inner(), &book)?;
    Ok(book.rules)
}

fn persist_rules(app: &AppHandle, state: &AppState, book: &RuleBook) -> AppResult<()> {
    save_rules(app, book)?;
    *state.rules.write().expect("rules poisoned") = book.clone();
    state.notify_monitor(MonitorCmd::Reload);
    Ok(())
}

// ---------------- 设置 ----------------

#[tauri::command]
pub fn get_settings(state: St<'_>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: St<'_>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    let mut next = state.settings_snapshot();
    let want_autostart = patch.autostart;
    patch.apply_to(&mut next);

    // 自启是副作用，失败要回滚标志位，别让 UI 骗人
    if let Some(enabled) = want_autostart {
        if let Err(e) = apply_autostart(&app, enabled) {
            next.autostart = !enabled;
            save_settings(&app, &next)?;
            *state.settings.write().expect("settings poisoned") = next;
            return Err(e);
        }
    }

    save_settings(&app, &next)?;
    *state.settings.write().expect("settings poisoned") = next.clone();
    state.notify_monitor(MonitorCmd::Reload);
    Ok(next)
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let res = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    res.map_err(|e| AppError::System(format!("设置开机自启失败: {e}")))
}

#[tauri::command]
pub fn is_autostart_enabled(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

// ---------------- 进程 ----------------

#[derive(Debug, Serialize)]
pub struct ProcRow {
    pub pid: u32,
    pub exe_name: String,
    pub path: Option<String>,
    pub window_title: String,
    pub instances: usize,
    /// 受保护进程（系统关键 / Windows 目录 / 自身链 / 黑名单）：可以被展示，但绝不能被处置
    pub is_protected: bool,
    /// 位于 Windows 目录下的系统自带程序
    pub is_system: bool,
}

/// 列出当前运行的进程，供用户挑规则目标（避免手打路径）。
///
/// 两个过滤开关默认都开：列表默认只留「正常可管控」的第三方程序，
/// 需要排查时再手动放开。
#[tauri::command]
pub fn list_running_processes(
    state: St<'_>,
    exclude_protected: Option<bool>,
    ignore_system: Option<bool>,
) -> Vec<ProcRow> {
    use crate::win::foreground::{resolve_image, ImageLookup};

    let exclude_protected = exclude_protected.unwrap_or(true);
    let ignore_system = ignore_system.unwrap_or(true);
    let extra = state.settings_snapshot().extra_protected;

    let guard = state.guard_snapshot();
    let entries = procs::snapshot_fresh();
    // 标题一次枚举拿到，避免逐进程重复扫描窗口树
    let titles = crate::win::close::window_titles();
    let mut rows: Vec<ProcRow> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for e in entries {
        if e.pid == 0 || e.pid == 4 || guard.contains(e.pid) {
            continue;
        }
        let key = e.exe_name.to_lowercase();
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }

        let lookup = resolve_image(e.pid);
        let path = match &lookup {
            ImageLookup::Full(p) => Some(p.clone()),
            _ => None,
        };
        // 判保护要用完整路径键，读不到路径时退化为进程名
        let exe_key = match &lookup {
            ImageLookup::Full(p) => crate::config::normalize_exe_key(p),
            _ => key.clone(),
        };
        let is_system = protected::is_system_path(&exe_key);
        let is_protected =
            protected::is_protected(&exe_key, &e.exe_name, e.pid, &guard, &extra);

        if exclude_protected && is_protected {
            continue;
        }
        if ignore_system && is_system {
            continue;
        }

        rows.push(ProcRow {
            pid: e.pid,
            instances: procs::count_by_name(&e.exe_name),
            exe_name: e.exe_name,
            path,
            window_title: titles.get(&e.pid).cloned().unwrap_or_default(),
            is_protected,
            is_system,
        });
    }
    rows.sort_by(|a, b| a.exe_name.to_lowercase().cmp(&b.exe_name.to_lowercase()));
    rows
}

/// 进入屏幕拾取模式：接下来点击的任意应用窗口会被解析成目标进程。
///
/// 钩子最长存活 15 秒，右键/Esc/超时都会自动取消；前端另有取消命令兜底。
#[tauri::command]
pub fn start_window_pick(app: AppHandle, state: St<'_>) {
    let guard = state.guard_snapshot();
    let extra = state.settings_snapshot().extra_protected;
    crate::win::picker::start(app, guard, extra, std::time::Duration::from_secs(15));
}

/// 取消屏幕拾取（前端退出/关闭弹窗时的兜底）。
#[tauri::command]
pub fn cancel_window_pick() {
    crate::win::picker::stop();
}

/// 一个程序的展示元信息。
#[derive(Debug, Clone, Serialize)]
pub struct AppMeta {
    /// 应用显示名（exe 的文件说明，读不到则回退文件名）
    pub name: String,
    /// PNG data URL，取不到图标时为 None
    pub icon: Option<String>,
}

fn looks_like_path(value: &str) -> bool {
    value.contains('\\') || value.contains('/')
}

/// 批量取程序图标与应用名。
///
/// 入参既可以是完整路径，也可以是纯 exe 名（规则 `pattern`、白/黑名单的常见形态）：
/// 纯名会先从当前进程表里解析真实路径，解析不到就用文件名兜底。
/// 提取在阻塞线程池完成，避免拖慢 IPC；结果在 `win::icon` 里按路径缓存。
#[tauri::command]
pub async fn get_app_meta(values: Vec<String>) -> HashMap<String, AppMeta> {
    use crate::win::foreground::{resolve_image, ImageLookup};

    let mut seen = HashSet::new();
    // 按原样去重：大小写不同的两条也要各自回一个 key，否则前端缺一条就永远空着。
    // 真正的重复提取由 `win::icon` 的大小写不敏感缓存挡掉。
    let unique: Vec<String> = values
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .filter(|v| seen.insert(v.clone()))
        .take(300)
        .collect();
    if unique.is_empty() {
        return HashMap::new();
    }

    let joined = tauri::async_runtime::spawn_blocking(move || {
        // 纯进程名 → 运行中同名进程的真实路径（拿不到就只能用文件名）
        let mut resolved: HashMap<String, String> = HashMap::new();
        let bare: Vec<String> = unique
            .iter()
            .filter(|v| !looks_like_path(v))
            .map(|v| v.to_ascii_lowercase())
            .collect();
        if !bare.is_empty() {
            for entry in procs::snapshot_fresh() {
                let name = entry.exe_name.to_ascii_lowercase();
                if !bare.contains(&name) || resolved.contains_key(&name) {
                    continue;
                }
                if let ImageLookup::Full(path) = resolve_image(entry.pid) {
                    resolved.insert(name, path);
                }
            }
        }

        let mut out = HashMap::new();
        for value in unique {
            let path = if looks_like_path(&value) {
                value.clone()
            } else {
                resolved
                    .get(&value.to_ascii_lowercase())
                    .cloned()
                    .unwrap_or_else(|| value.clone())
            };
            let icon = crate::win::icon::data_url(&path);
            let name = crate::win::icon::app_name(&path).unwrap_or_else(|| {
                std::path::Path::new(&value)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| value.clone())
            });
            out.insert(value, AppMeta { name, icon });
        }
        out
    })
    .await;

    joined.ok().unwrap_or_default()
}

// ---------------- 统计 ----------------

#[tauri::command]
pub fn query_day_stats(state: St<'_>, from: String, to: String) -> AppResult<Vec<queries::DayStat>> {
    queries::day_stats(&state.db, &from, &to)
}

#[tauri::command]
pub fn query_day_detail(state: St<'_>, day: String) -> AppResult<Vec<queries::ExeStat>> {
    queries::day_detail(&state.db, &day)
}

#[tauri::command]
pub fn query_top_exes(
    state: St<'_>,
    from: String,
    to: String,
    limit: usize,
) -> AppResult<Vec<queries::ExeStat>> {
    queries::top_exes(&state.db, &from, &to, limit.clamp(1, 50))
}

#[tauri::command]
pub fn query_grand_total(state: St<'_>) -> AppResult<i64> {
    queries::grand_total_ms(&state.db)
}

/// 历史分类统计：专注 / 摸鱼 / 专注摸鱼，三类互斥且相加等于当日去重总时长。
#[tauri::command]
pub fn query_day_category_stats(
    state: St<'_>,
    from: String,
    to: String,
) -> AppResult<Vec<queries::DayCategoryStat>> {
    let focus_rule_ids: Vec<String> = state
        .rules_snapshot()
        .rules
        .into_iter()
        .filter(|r| r.kind == RuleKind::Focus)
        .map(|r| r.id)
        .collect();
    queries::day_category_stats(&state.db, &from, &to, &focus_rule_ids)
}

/// 过滤 Windows 文件名非法字符，避免 from/to 拼进路径出错。
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// 重名时追加 -1/-2 后缀，绝不覆盖用户已有文件。
fn unique_export_path(dir: &std::path::Path, stem: &str, ext: &str) -> std::path::PathBuf {
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut i = 1;
    while path.exists() && i < 1000 {
        path = dir.join(format!("{stem}-{i}.{ext}"));
        i += 1;
    }
    path
}

/// 导出 CSV 到系统下载文件夹并返回完整路径（导出后自动在资源管理器中选中）。
#[tauri::command]
pub fn export_csv(app: AppHandle, state: St<'_>, from: String, to: String) -> AppResult<String> {
    use tauri_plugin_opener::OpenerExt;

    let rows = queries::day_stats(&state.db, &from, &to)?;
    let mut out = String::from("date,total_minutes\n");
    for r in rows {
        out.push_str(&format!("{},{}\n", r.day, r.total_ms / 60_000));
    }

    let dir = crate::config::download_dir(&app)?;
    let stem = format!(
        "florime-stats-{}-{}",
        sanitize_filename(&from),
        sanitize_filename(&to)
    );
    let path = unique_export_path(&dir, &stem, "csv");
    std::fs::write(&path, out)?;
    log::info!("统计已导出到 {}", path.to_string_lossy());

    // Rust 侧调用不受前端 capability 限制，直接定位到下载的文件
    if let Err(e) = app.opener().reveal_item_in_dir(&path) {
        log::warn!("在资源管理器中定位导出文件失败: {e}");
    }
    Ok(path.to_string_lossy().to_string())
}

/// 在资源管理器中定位数据目录（用 reveal 而非 open_path，避免缺少 opener 权限）。
#[tauri::command]
pub fn open_data_dir(app: AppHandle) -> AppResult<String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = crate::config::data_dir(&app)?;
    app.opener()
        .reveal_item_in_dir(&dir)
        .map_err(|e| AppError::System(e.to_string()))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 在资源管理器中定位规则对应程序的目录。
/// 纯进程名先从运行中的进程解析真实路径，解析不到才按文件名处理。
#[tauri::command]
pub async fn reveal_rule_path(app: AppHandle, pattern: String) -> AppResult<()> {
    use crate::win::foreground::{resolve_image, ImageLookup};
    use tauri_plugin_opener::OpenerExt;

    let pattern = pattern.trim().to_string();
    if pattern.is_empty() {
        return Err(AppError::System("规则路径为空".to_string()));
    }

    let path = if looks_like_path(&pattern) {
        std::path::PathBuf::from(&pattern)
    } else {
        let bare = pattern.to_ascii_lowercase();
        let resolved = tauri::async_runtime::spawn_blocking(move || -> Option<String> {
            for entry in procs::snapshot_fresh() {
                if entry.exe_name.to_ascii_lowercase() != bare {
                    continue;
                }
                if let ImageLookup::Full(path) = resolve_image(entry.pid) {
                    return Some(path);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
        resolved
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(&pattern))
    };

    if !path.exists() {
        return Err(AppError::System(format!("路径不存在: {}", path.display())));
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| AppError::System(e.to_string()))
}

#[tauri::command]
pub fn db_path(app: AppHandle) -> String {
    crate::config::data_dir(&app)
        .map(|d| d.join("focus.db").to_string_lossy().to_string())
        .unwrap_or_default()
}

// ---------------- 系统 ----------------

#[tauri::command]
pub fn is_elevated() -> bool {
    crate::win::is_elevated()
}

/// 以管理员身份重启自己（处置高权限进程时才需要）。
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> AppResult<()> {
    if let Some(h) = app.try_state::<Arc<AppState>>() {
        h.notify_monitor(MonitorCmd::Shutdown);
    }
    crate::win::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ExceedAction, MatchKind, MonitorMode, NotifyConfig, RuleKind};

    fn rule(id: &str) -> Rule {
        Rule {
            id: id.into(),
            display_name: format!("规则 {id}"),
            match_kind: MatchKind::ExeName,
            pattern: "test.exe".into(),
            mode: MonitorMode::Foreground,
            daily_limit_secs: 60,
            grace_secs: 15,
            on_exceed: ExceedAction::NotifyOnly,
            allow_background: false,
            enabled: true,
            paused: false,
            kind: RuleKind::Slack,
            rest_remind_secs: 0,
            rest_auto_continue: true,
            rest: NotifyConfig::default(),
            exceed_remind: true,
            exceed: NotifyConfig::default(),
        }
    }

    fn ids(rules: &[Rule]) -> Vec<&str> {
        rules.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn reorder_follows_ids_and_appends_the_rest() {
        let mut rules = vec![rule("a"), rule("b"), rule("c")];
        apply_rule_order(&mut rules, &["c".into(), "a".into()]);
        assert_eq!(ids(&rules), vec!["c", "a", "b"]);
    }

    #[test]
    fn reorder_ignores_unknown_and_duplicate_ids() {
        let mut rules = vec![rule("a"), rule("b")];
        // 未知 id 忽略；重复 id 只取第一次；未列出的 a 保留并补在末尾
        apply_rule_order(&mut rules, &["b".into(), "zzz".into(), "b".into()]);
        assert_eq!(ids(&rules), vec!["b", "a"]);
    }

    #[test]
    fn reorder_with_empty_ids_keeps_original_order() {
        let mut rules = vec![rule("a"), rule("b"), rule("c")];
        apply_rule_order(&mut rules, &[]);
        assert_eq!(ids(&rules), vec!["a", "b", "c"]);
    }
}


