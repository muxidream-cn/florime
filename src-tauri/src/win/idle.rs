//! 用户空闲检测。超过阈值说明人不在，不应继续计时。

use std::time::Duration;

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// 距最后一次键鼠输入过了多久。
pub fn idle_duration() -> Duration {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut info).as_bool() {
            return Duration::ZERO;
        }
        let now = GetTickCount();
        // dwTime / GetTickCount 都是 u32，49 天回绕，用 wrapping_sub 处理
        let idle_ms = now.wrapping_sub(info.dwTime);
        Duration::from_millis(u64::from(idle_ms))
    }
}
