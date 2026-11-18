//! 强制结束进程。
//!
//! 这一层只做"能不能杀"，"该不该杀"由调用方（五道闸）负责。

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

/// 终止请求下发后最多等这么久让进程真正退出。
///
/// `TerminateProcess` 是异步语义：返回成功只代表请求已下发，进程对象要等内核回收后
/// 才从进程表消失。调用方（处置收尾）紧接着会扫描残留，不等待就会把「正在终止」
/// 误判成「还在运行」，弹出假的「还剩 N 个进程未关闭」。
const TERMINATE_WAIT_MS: u32 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillOutcome {
    Killed,
    NotFound,
    /// 权限不足（高完整性/PPL 进程必然如此）
    AccessDenied,
    Failed(u32),
}

/// 结束单个进程。不重试、不假装成功。
pub fn terminate(pid: u32) -> KillOutcome {
    if pid == 0 || pid == 4 {
        return KillOutcome::Failed(5); // ERROR_ACCESS_DENIED
    }
    unsafe {
        // 优先带 SYNCHRONIZE（后面要等退出）；权限不足时退回最小权限，别把能杀的目标变成拒绝
        let handle = match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
            .or_else(|_| OpenProcess(PROCESS_TERMINATE, false, pid))
        {
            Ok(h) => h,
            Err(_) => {
                return if crate::win::procs::exists(pid) {
                    KillOutcome::AccessDenied
                } else {
                    KillOutcome::NotFound
                };
            }
        };
        let r = TerminateProcess(handle, 1);
        let err = windows::Win32::Foundation::GetLastError();

        if r.is_ok() {
            // 等它真的从进程表里消失；超时不算失败，终止请求已经下发过了
            let _ = WaitForSingleObject(handle, TERMINATE_WAIT_MS);
        }
        let _ = CloseHandle(handle);

        if r.is_ok() {
            KillOutcome::Killed
        } else if !crate::win::procs::exists(pid) {
            // 目标已自行退出，也算成功
            KillOutcome::Killed
        } else {
            KillOutcome::Failed(err.0)
        }
    }
}
