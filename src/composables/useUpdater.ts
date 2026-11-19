import { relaunch } from "@tauri-apps/plugin-process"
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater"
import { computed, readonly, shallowRef, type ComputedRef, type Ref } from "vue"

const AUTO_CHECK_KEY = "florime.update.autoCheck"

export type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error"

export interface UpdateProgress {
  type: "started" | "progress" | "finished"
  downloaded: number
  total: number
}

/** 从 latest.json（update.rawJson）解析出的更新信息，字段与 CI 产出对齐 */
export interface UpdateInfo {
  version: string
  currentVersion: string
  notes: string
  pubDate: string
  size: number
  forceUpdate: boolean
}

const emptyUpdateInfo = (): UpdateInfo => ({
  version: "",
  currentVersion: "",
  notes: "",
  pubDate: "",
  size: 0,
  forceUpdate: false,
})

/** 全部为模块级单例状态：设置页与全局对话框共享同一实例。 */
const status = shallowRef<UpdateStatus>("idle")
const dialogOpen = shallowRef(false)
const checking = shallowRef(false)
const autoCheck = shallowRef(readAutoCheck())
const progress = shallowRef<UpdateProgress>({ type: "started", downloaded: 0, total: 0 })
const error = shallowRef("")
const updateInfo = shallowRef<UpdateInfo>(emptyUpdateInfo())
const downloadFailureCount = shallowRef(0)
/** Update 是持有 Rust 侧资源的句柄，不能放进 reactive 代理，仅模块内可见。 */
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
 * @param value
 */
function asNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 0
}

/**
 *
 * @param update
 */
function parseUpdateInfo(update: Update): UpdateInfo {
  const raw = update.rawJson ?? {}
  return {
    version: update.version,
    currentVersion: update.currentVersion,
    notes: typeof raw.notes === "string" ? raw.notes : (update.body ?? ""),
    pubDate: typeof raw.pub_date === "string" ? raw.pub_date : (update.date ?? ""),
    size: asNumber(raw.size),
    forceUpdate: raw.force_update === true,
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
    updateInfo.value = parseUpdateInfo(update)
    downloadFailureCount.value = 0
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
    downloadFailureCount.value = 0
  } catch (e) {
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    downloadFailureCount.value += 1
    throw e
  }
}

/** 下载并安装：一步到位，Windows 上 install 会启动安装器并退出应用 */
async function downloadAndInstall(): Promise<void> {
  if (!currentUpdate) return

  status.value = "downloading"
  resetProgress()
  try {
    await currentUpdate.downloadAndInstall(handleDownloadEvent)
    status.value = "ready"
    downloadFailureCount.value = 0
    dialogOpen.value = false
    // macOS/Linux 需要显式重启；Windows 上 install 已接管重启，调用通常不会返回
    await relaunch().catch(() => undefined)
  } catch (e) {
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    downloadFailureCount.value += 1
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
    await currentUpdate.install()
    status.value = "ready"
    dialogOpen.value = false
    await relaunch().catch(() => undefined)
  } catch (e) {
    status.value = "error"
    error.value = e instanceof Error ? e.message : String(e)
    downloadFailureCount.value += 1
    throw e
  }
}

/**
 *
 */
function cancel(): void {
  dialogOpen.value = false
  releaseUpdate()
  status.value = "idle"
  resetProgress()
}

const isBusy = computed(() => status.value === "downloading" || status.value === "ready")

export interface UseUpdaterReturn {
  status: Readonly<Ref<UpdateStatus>>
  dialogOpen: Readonly<Ref<boolean>>
  checking: Readonly<Ref<boolean>>
  autoCheck: Readonly<Ref<boolean>>
  updateInfo: Readonly<Ref<UpdateInfo>>
  progress: Readonly<Ref<UpdateProgress>>
  error: Readonly<Ref<string>>
  downloadFailureCount: Readonly<Ref<number>>
  isBusy: ComputedRef<boolean>
  setAutoCheck: (value: boolean) => void
  checkForUpdate: (silent?: boolean) => Promise<boolean>
  updateApp: () => Promise<void>
  downloadAndInstall: () => Promise<void>
  installApp: () => Promise<void>
  cancel: () => void
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
    updateInfo: readonly(updateInfo),
    progress: readonly(progress),
    error: readonly(error),
    downloadFailureCount: readonly(downloadFailureCount),
    isBusy,
    setAutoCheck,
    checkForUpdate,
    updateApp,
    downloadAndInstall,
    installApp,
    cancel,
  }
}
