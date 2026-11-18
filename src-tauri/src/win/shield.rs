//! 全屏提醒期间的切出快捷键屏蔽。
//!
//! 只拦一组会离开当前窗口的组合键（Win、Alt+Tab、Alt+F4、Ctrl+Esc 等），
//! 不是全局键盘封锁。任何路径（倒计时结束、用户关闭、暂停、退出）都必须卸载，
//! 再加看门狗兜底，绝不允许把用户锁死在提醒里。
//!
//! `Ctrl+Alt+Del`、`Win+L` 等系统保留组合由内核直接处理，钩子收不到，属正常现象。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_F4, VK_LWIN, VK_MENU, VK_RWIN,
    VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_QUIT,
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// 每次 start/stop 递增；钩子线程与看门狗都靠它判断自己是否已过期
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn pressed(key: VIRTUAL_KEY) -> bool {
    // 返回值高位为 1 表示当前按下
    unsafe { GetAsyncKeyState(i32::from(key.0)) as u16 & 0x8000 != 0 }
}

/// 是否属于「离开当前窗口」的组合键。
fn should_block(vk: u32) -> bool {
    if vk == u32::from(VK_LWIN.0) || vk == u32::from(VK_RWIN.0) {
        // 单按 Win 就能拉起开始菜单/组合键，直接吞掉
        return true;
    }
    let alt = pressed(VK_MENU);
    let ctrl = pressed(VK_CONTROL);
    if vk == u32::from(VK_TAB.0) {
        return alt;
    }
    if vk == u32::from(VK_ESCAPE.0) {
        return alt || ctrl;
    }
    if vk == u32::from(VK_F4.0) {
        return alt;
    }
    if vk == u32::from(VK_SPACE.0) {
        return alt;
    }
    false
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && ACTIVE.load(Ordering::Relaxed) {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if should_block(info.vkCode) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// 启用屏蔽。重复调用只延长/刷新看门狗，不会叠加钩子。
pub fn start(duration: Duration) {
    // 与屏幕拾取互斥：两套低级钩子同时在线会互相吞事件
    crate::win::picker::stop();

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    if !ACTIVE.swap(true, Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("florime-shield".into())
            .spawn(move || unsafe {
                let mut msg = MSG::default();
                // 先建消息队列，否则 PostThreadMessageW 可能投递失败
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);

                let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        log::error!("安装键盘屏蔽钩子失败: {e}");
                        if GENERATION.load(Ordering::SeqCst) == generation {
                            ACTIVE.store(false, Ordering::SeqCst);
                        }
                        return;
                    }
                };

                THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);

                // 安装期间可能已经被 stop（用户秒点关闭）：此时没人能给我们发 WM_QUIT，
                // 必须自己卸载退出，否则钩子会留下一个无法回收的空壳。
                if GENERATION.load(Ordering::SeqCst) != generation {
                    log::info!("键盘屏蔽在就绪前已被取消，直接卸载");
                    let _ = UnhookWindowsHookEx(hook);
                    THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }

                log::info!("键盘屏蔽已启用（最长 {} 秒）", duration.as_secs());

                // WM_QUIT 返回 0，出错返回 -1：两种情况都必须退出，否则会忙等
                loop {
                    if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                        break;
                    }
                }

                let _ = UnhookWindowsHookEx(hook);
                if GENERATION.load(Ordering::SeqCst) == generation {
                    THREAD_ID.store(0, Ordering::SeqCst);
                    ACTIVE.store(false, Ordering::SeqCst);
                }
                log::info!("键盘屏蔽已解除");
            });

        if spawned.is_err() {
            ACTIVE.store(false, Ordering::SeqCst);
            log::error!("无法启动键盘屏蔽线程");
        }
    }

    // 看门狗：不管调用方给了多长，到点一定解除
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        if GENERATION.load(Ordering::SeqCst) == generation {
            log::warn!("键盘屏蔽到达时限，强制解除");
            stop();
        }
    });
}

/// 解除屏蔽；幂等，任何收敛路径都可以无脑调用。
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
}
