<script setup lang="ts">
import { Crosshair, FolderOpen, FolderSearch, List } from "@lucide/vue"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { open } from "@tauri-apps/plugin-dialog"
import { onBeforeUnmount, ref } from "vue"

import { cancelWindowPick, startWindowPick } from "@/api/commands"
import { EV, useTauriEvent } from "@/api/events"
import ProcessPicker from "@/components/ProcessPicker.vue"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"

import type { PickedWindow, ProcRow } from "@/types/models"

/**
 * 规则目标的输入与三种选择入口。
 *
 * 三种方式各有取舍：
 * - 选文件：目标不在运行时也能建规则，但需要自己找到 exe；
 * - 拾取窗口：最直观，取到的是「正在屏幕上的那个进程」；
 * - 进程列表：目标已在运行时最稳，还能看到受保护标记。
 *
 * 拾取期间主窗会最小化：鼠标钩子会吞掉点击，主窗若挡在前面用户就点不到目标了。
 * 组件只负责把选中结果抛给父层，`pattern / match_kind / 名称兜底` 由父层统一写入。
 */
const pattern = defineModel<string>("pattern", { required: true })
const emit = defineEmits<{ picked: [row: ProcRow] }>()

const pickerOpen = ref(false)
/** 拾取进行中：主窗已最小化，等待后端钩子回事件 */
const picking = ref(false)
/** 行内提示：选中结果、取消原因、失败信息 */
const hint = ref("")

/**
 *
 * @param window
 */
function toRow(window: PickedWindow): ProcRow {
  return {
    pid: window.pid,
    exe_name: window.exe_name,
    path: window.path,
    window_title: window.title,
    instances: 1,
    is_protected: window.is_protected,
    is_system: window.is_system,
  }
}

/**
 *
 */
async function restoreWindow(): Promise<void> {
  try {
    const w = getCurrentWindow()
    await w.unminimize()
    await w.setFocus()
  } catch {
    /* 窗口可能已销毁 */
  }
}

/**
 *
 */
async function chooseFile(): Promise<void> {
  hint.value = ""
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      title: "选择要管理的程序",
      filters: [{ name: "可执行文件", extensions: ["exe"] }],
    })
    if (typeof picked !== "string") return
    const name = picked.split(/[\\/]/).pop() ?? picked
    emit("picked", {
      pid: 0,
      exe_name: name,
      path: picked,
      window_title: "",
      instances: 1,
      is_protected: false,
      is_system: false,
    })
    hint.value = `已选择 ${name}`
  } catch (e) {
    console.error("选择 exe 文件失败", e)
    hint.value = "打开文件选择器失败，请改用其他方式"
  }
}

/**
 *
 */
async function pickWindow(): Promise<void> {
  if (picking.value) return
  hint.value = ""
  picking.value = true
  try {
    // 必须先最小化：钩子会吞掉点击，主窗挡着就点不到目标窗口
    await getCurrentWindow().minimize()
    await startWindowPick()
  } catch (e) {
    console.error("启动窗口拾取失败", e)
    picking.value = false
    await restoreWindow()
    hint.value = "无法启动窗口拾取，请改用其他方式"
  }
}

useTauriEvent<PickedWindow>(EV.WINDOW_PICKED, (payload) => {
  picking.value = false
  void restoreWindow().then(() => {
    const row = toRow(payload)
    emit("picked", row)
    if (payload.is_protected) {
      hint.value = `已选中 ${payload.exe_name}（受保护进程，不能被处置）`
    } else if (!payload.path) {
      hint.value = `${payload.exe_name} 读不到完整路径，只能按进程名匹配`
    } else {
      hint.value = `已选中 ${payload.exe_name}`
    }
  })
})

/** 取消原因来自后端：超时要明确告知，否则用户以为是自己点错了 */
const CANCEL_REASON: Record<string, string> = {
  timeout: "15 秒内没有选中窗口，拾取已取消",
  escape: "已按 Esc 取消拾取",
  right_click: "已按右键取消拾取",
}

useTauriEvent<string>(EV.WINDOW_PICK_CANCELLED, (reason) => {
  picking.value = false
  void restoreWindow().then(() => {
    hint.value = CANCEL_REASON[reason] ?? "已取消窗口拾取"
  })
})

/**
 *
 * @param row
 */
function onListPicked(row: ProcRow): void {
  hint.value = row.path ? `已选中 ${row.exe_name}` : `已选中 ${row.exe_name}（无完整路径）`
  emit("picked", row)
}

onBeforeUnmount(() => {
  // 弹窗被关掉时钩子必须收掉，否则鼠标会一直被吞
  if (picking.value) {
    picking.value = false
    void cancelWindowPick()
    void restoreWindow()
  }
})
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <div class="flex gap-2">
      <Input v-model="pattern" placeholder="C:\Path\App.exe 或 app.exe" class="h-8 flex-1 font-mono text-[0.8125rem]" />
      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <Button size="sm" variant="outline" class="cursor-pointer">
            <FolderSearch data-icon="inline-start" />
            选择
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" class="w-52">
          <DropdownMenuItem class="cursor-pointer gap-2 text-xs" @select="chooseFile">
            <FolderOpen class="text-muted-foreground size-3.5" />
            从目录选择 exe
          </DropdownMenuItem>
          <DropdownMenuItem class="cursor-pointer gap-2 text-xs" @select="pickWindow">
            <Crosshair class="text-muted-foreground size-3.5" />
            点击屏幕窗口拾取
          </DropdownMenuItem>
          <DropdownMenuItem class="cursor-pointer gap-2 text-xs" @select="pickerOpen = true">
            <List class="text-muted-foreground size-3.5" />
            从运行进程选择
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>

    <p v-if="picking" class="flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
      <Crosshair class="size-3 shrink-0 animate-pulse" />
      窗口已最小化：请在屏幕上点击目标窗口，右键 / Esc 取消
    </p>
    <p v-else-if="hint" class="text-muted-foreground text-xs">{{ hint }}</p>
  </div>

  <ProcessPicker v-model:open="pickerOpen" @select="onListPicked" />
</template>
