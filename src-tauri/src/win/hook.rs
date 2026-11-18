//! 前台切换事件钩子。
//!
//! `WINEVENT_OUTOFCONTEXT` 的回调通过安装钩子那个线程的消息队列投递，
//! 所以必须有 GetMessage 泵，且回调要 < 1s 返回 —— 这里只做 try_send，
//! 绝不 OpenProcess / 不写库 / 不 emit。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK, WINEVENTPROC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage,
    EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_QUIT,
};

static SENDER: OnceLock<Mutex<Option<Sender<isize>>>> = OnceLock::new();

fn sender_slot() -> &'static Mutex<Option<Sender<isize>>> {
    SENDER.get_or_init(|| Mutex::new(None))
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _idobject: i32,
    _idchild: i32,
    _ideventthread: u32,
    _eventtime: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || hwnd.0.is_null() {
        return;
    }
    if let Ok(guard) = sender_slot().lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(hwnd.0 as isize);
        }
    }
}

pub struct HookHandle {
    thread_id: Arc<AtomicU32>,
    join: Option<JoinHandle<()>>,
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        let tid = self.thread_id.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(join) = self.join.take() {
            // 最多等 2 秒，超时就放弃，避免卡死退出流程
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = join.join();
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
        }
        if let Ok(mut g) = sender_slot().lock() {
            *g = None;
        }
    }
}

/// 启动前台切换监听。事件通过 `tx` 投递（内容是 HWND 的 isize）。
pub fn spawn_foreground_hook(tx: Sender<isize>) -> HookHandle {
    if let Ok(mut g) = sender_slot().lock() {
        *g = Some(tx);
    }
    let thread_id = Arc::new(AtomicU32::new(0));
    let tid_out = thread_id.clone();

    let join = std::thread::Builder::new()
        .name("florime-winevent".into())
        .spawn(move || unsafe {
            tid_out.store(GetCurrentThreadId(), Ordering::SeqCst);

            let proc: WINEVENTPROC = Some(win_event_proc);
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                proc,
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, None, 0, 0);
                if ret.0 == 0 || ret.0 == -1 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if !hook.0.is_null() {
                let _ = UnhookWinEvent(hook);
            }
        })
        .expect("无法创建 WinEvent 线程");

    HookHandle {
        thread_id,
        join: Some(join),
    }
}
