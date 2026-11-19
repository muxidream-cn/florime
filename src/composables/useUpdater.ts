import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater"
import { computed, readonly, shallowRef, type ComputedRef, type Ref } from "vue"

const AUTO_CHECK_KEY = "florime.update.autoCheck"

export type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error"

export interface UpdateProgress {
  type: "started" | "progress" | "finished"
  downloaded: number
  total: number
}

/** 全部为模块级单例状态：设置页与全局对话框共享同一实例。 */
const status = shallowRef<UpdateStatus>("idle")
const dialogOpen = shallowRef(false)
const checking = shallowRef(false)
const autoCheck = shallowRef(readAutoCheck())
const newVersion = shallowRef("")
const newVersionBody = shallowRef("")
const progress = shallowRef<UpdateProgress>({ type: "started", downloaded: 0, total: 0 })
const error = shallowRef("")
/** Update 是持有 Rust 侧资源的句柄，不能放进 reactive 代理，故用 shallowRef 且仅模块内可见。 */
let currentUpdate: Update | null = null

/**
 *
 */
function readAutoCheck(): boolean {
  try {
    return localStorage.getItem(AUTO_CHECK_KEY) === "1"
  } catch {
    return false
  }
}

/**
 *
 * @param value
 */
function setAutoCheck(value: boolean): void {
  autoCheck.value = value
  try {
    localStorage.setItem(AUTO_CHECK_KEY, value ? "1" : "0")
  } catch {
    // 隐私模式下 localStorage 可能不可用，偏好仅在本次会话生效
  }
}

/**
 *
 */
function resetProgress(): void {
  progress.value = { type: "started", downloaded: 0, total: 0 }
}

/**
 *
 * @param event
 */
function handleDownloadEvent(event: DownloadEvent): void {
  if (event.event === "Started") {
    progress.value = { type: "started", downloaded: 0, total: event.data?.contentLength ?? 0 }
  } else if (event.event === "Progress") {
    progress.value = {
      type: "progress",
      downloaded: progress.value.downloaded + event.data.chunkLength,
      total: progress.value.total,
    }
  } else {
    progress.value = { ...progress.value, type: "finished" }
  }
}

/**
 *
 */
function releaseUpdate(): void {
  if (!currentUpdate) return
  void currentUpdate.close()
  currentUpdate = null
}

/**
 *
 * @param silent
 */
async function checkForUpdate(silent = false): Promise<boolean> {
  if (checking.value) return false

  checking.value = true
  status.value = "checking"
  error.value = ""
  if (!silent) resetProgress()

  try {
    const update = await check()
    if (!update) {
      releaseUpdate()
      status.value = "idle"
      return false
    }

    releaseUpdate()
    currentUpdate = update
    newVersion.value = update.version
    newVersionBody.value = update.body ?? ""
    resetProgress()
    status.value = "available"
    dialogOpen.value = true
    return true
  } catch (e) {
    releaseUpdate()
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    throw e
  } finally {
    checking.value = false
  }
}

/**
 *
 */
async function updateApp(): Promise<void> {
  if (!currentUpdate) return

  status.value = "downloading"
  resetProgress()
  try {
    await currentUpdate.download(handleDownloadEvent)
    status.value = "ready"
  } catch (e) {
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    throw e
  }
}

/**
 *
 */
async function installApp(): Promise<void> {
  if (!currentUpdate) return

  status.value = "downloading"
  try {
    // Windows 上 install() 会启动安装器并退出应用
    await currentUpdate.install()
    status.value = "ready"
    dialogOpen.value = false
  } catch (e) {
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    throw e
  }
}

/**
 *
 */
function dismissDialog(): void {
  dialogOpen.value = false
  if (status.value !== "downloading" && status.value !== "ready") {
    releaseUpdate()
    status.value = "idle"
  }
}

const isBusy = computed(() => status.value === "downloading" || status.value === "ready")

export interface UseUpdaterReturn {
  status: Readonly<Ref<UpdateStatus>>
  dialogOpen: Readonly<Ref<boolean>>
  checking: Readonly<Ref<boolean>>
  autoCheck: Readonly<Ref<boolean>>
  newVersion: Readonly<Ref<string>>
  newVersionBody: Readonly<Ref<string>>
  progress: Readonly<Ref<UpdateProgress>>
  error: Readonly<Ref<string>>
  isBusy: ComputedRef<boolean>
  setAutoCheck: (value: boolean) => void
  checkForUpdate: (silent?: boolean) => Promise<boolean>
  updateApp: () => Promise<void>
  installApp: () => Promise<void>
  dismissDialog: () => void
}

/**
 *
 */
export function useUpdater(): UseUpdaterReturn {
  return {
    status: readonly(status),
    dialogOpen: readonly(dialogOpen),
    checking: readonly(checking),
    autoCheck: readonly(autoCheck),
    newVersion: readonly(newVersion),
    newVersionBody: readonly(newVersionBody),
    progress: readonly(progress),
    error: readonly(error),
    isBusy,
    setAutoCheck,
    checkForUpdate,
    updateApp,
    installApp,
    dismissDialog,
  }
}
