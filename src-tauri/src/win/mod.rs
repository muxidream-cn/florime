//! Windows 平台采集层。整个 crate 仅面向 Windows，无跨平台桩。

pub mod close;
pub mod foreground;
pub mod hook;
pub mod icon;
pub mod idle;
pub mod kill;
pub mod picker;
pub mod procs;
pub mod protected;
pub mod shield;

/// 是否以管理员身份运行。决定能否处置高权限目标进程。
pub fn is_elevated() -> bool {
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    unsafe { IsUserAnAdmin().as_bool() }
}

/// 系统提示音，不引入音频资源。
pub fn beep() {
    use windows::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING;
    unsafe {
        let _ = MessageBeep(MB_ICONWARNING);
    }
}

/// 提权重启时传给新实例的参数。
///
/// 提权实例启动时旧实例还没退出（必须先 ShellExecuteW 再自己退出），
/// 若新实例照常走单实例逻辑，它会认定旧实例是主实例、把启动请求转过去然后退出，
/// 紧接着旧实例也退出——应用就"消失"了。带这个参数启动时跳过单实例把门。
pub const ELEVATED_FLAG: &str = "--elevated-start";

/// 以管理员身份重新拉起自身（UAC 提权）。
///
/// 只有处置高完整性目标时才需要，平时不要提权——提权后自启和普通交互都会变麻烦。
pub fn relaunch_elevated() -> crate::error::AppResult<()> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe()
        .map_err(|e| crate::error::AppError::System(format!("取自身路径失败: {e}")))?;
    let verb = HSTRING::from("runas");
    let file = HSTRING::from(exe.to_string_lossy().to_string());
    // 参数用 PCWSTR 传：`Option<&HSTRING>` 在这个 windows 版本上不满足 Param 约束
    let params: Vec<u16> = ELEVATED_FLAG
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let hinst = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &file,
            windows::core::PCWSTR(params.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 返回值 > 32 表示成功
    if (hinst.0 as isize) <= 32 {
        return Err(crate::error::AppError::System(
            "提权重启被取消或失败".into(),
        ));
    }
    Ok(())
}
