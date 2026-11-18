import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { onUnmounted } from "vue"

/** 事件名，与 Rust `events.rs` 一一对应。 */
export const EV = {
  TICK: "focus://tick",
  ALERT: "focus://alert",
  ALERT_CLOSED: "focus://alert-closed",
  STATE: "focus://state",
  /** 专注模式的休息提醒 */
  REST: "focus://rest",
  REST_CLOSED: "focus://rest-closed",
  /** 休息结束后的继续努力确认 */
  REST_CONFIRM: "focus://rest-confirm",
  /** 屏幕拾取窗口：选中目标 / 取消 */
  WINDOW_PICKED: "focus://window-picked",
  WINDOW_PICK_CANCELLED: "focus://window-pick-cancelled",
} as const

/**
 * 订阅 Tauri 事件，组件卸载时自动解绑。
 *
 * 必须在组件 setup 期间调用（依赖 onUnmounted）。
 * @param name
 * @param cb
 */
export function useTauriEvent<T>(name: string, cb: (payload: T) => void): void {
  let unlisten: UnlistenFn | null = null
  let disposed = false

  listen<T>(name, (event) => cb(event.payload))
    .then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })
    .catch((err) => {
      console.error(`订阅事件 ${name} 失败`, err)
    })

  onUnmounted(() => {
    disposed = true
    unlisten?.()
  })
}
