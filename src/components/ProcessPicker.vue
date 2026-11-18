<script setup lang="ts">
import { AppWindow, Check, Search } from "@lucide/vue"
import { computed, ref, watch } from "vue"

import { listRunningProcesses } from "@/api/commands"
import MiddleEllipsis from "@/components/MiddleEllipsis.vue"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { useAppMeta } from "@/composables/useAppMeta"

import type { ProcRow } from "@/types/models"

const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ select: [row: ProcRow] }>()

const rows = ref<ProcRow[]>([])
const loading = ref(false)
/** 图标与应用名：按 exe 路径共享缓存 */
const { ensure, iconOf, nameOf } = useAppMeta()
const keyword = ref("")
/** 默认只留「正常可管控」的第三方程序，需要排查时再放开 */
const excludeProtected = ref(true)
const ignoreSystem = ref(true)
/** 读不到 exe 路径的进程建不了「完整路径」规则，默认挡掉 */
const hideUnreadable = ref(true)
const useRegex = ref(false)

interface Segment {
  text: string
  hit: boolean
}

/**
 *
 * @param input
 */
function escapeRegExp(input: string): string {
  return input.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

const regexError = computed(() => {
  const k = keyword.value.trim()
  if (!k || !useRegex.value) return ""
  try {
    new RegExp(k)
    return ""
  } catch {
    return "正则表达式无效，改好之前不显示结果"
  }
})

/** 高亮与过滤共用同一个表达式；正则出错时置空，避免按半截输入误匹配 */
const searchRe = computed<RegExp | null>(() => {
  const k = keyword.value.trim()
  if (!k || regexError.value) return null
  return new RegExp(useRegex.value ? k : escapeRegExp(k), "gi")
})

/**
 * 元信息按「完整路径优先」索引：路径读不到时退回 exe 名
 * @param row
 */
function metaKey(row: ProcRow): string {
  return row.path ?? row.exe_name
}

/**
 * 主标题用应用名，取不到就回退 exe 名
 * @param row
 */
function appLabel(row: ProcRow): string {
  return nameOf(metaKey(row), row.exe_name)
}

const filtered = computed(() => {
  const k = keyword.value.trim()
  if (!k) return rows.value
  const re = searchRe.value
  if (!re) return []
  // 每次用新的非全局表达式做 test，避免 lastIndex 串位
  const probe = new RegExp(re.source, "i")
  return rows.value.filter((r) => probe.test(r.exe_name) || probe.test(appLabel(r)) || probe.test(r.window_title) || probe.test(r.path ?? ""))
})

/**
 * 把命中的片段切出来，模板里用 mark 标红
 * @param text
 */
function segments(text: string): Segment[] {
  const re = searchRe.value
  if (!re || !text) return [{ text, hit: false }]
  const scan = new RegExp(re.source, "gi")
  const out: Segment[] = []
  let last = 0
  for (const m of text.matchAll(scan)) {
    const matched = m[0]
    if (!matched) continue
    const start = m.index ?? 0
    if (start > last) out.push({ text: text.slice(last, start), hit: false })
    out.push({ text: matched, hit: true })
    last = start + matched.length
  }
  if (last < text.length) out.push({ text: text.slice(last), hit: false })
  return out.length > 0 ? out : [{ text, hit: false }]
}

/**
 *
 */
async function refresh(): Promise<void> {
  loading.value = true
  try {
    rows.value = await listRunningProcesses({
      excludeProtected: excludeProtected.value,
      ignoreSystem: ignoreSystem.value,
    })
  } catch (e) {
    console.error("读取进程列表失败", e)
    rows.value = []
  } finally {
    loading.value = false
  }
  // 图标/应用名异步回填，先出列表不卡界面
  ensure(rows.value.map(metaKey))
}

const visibleRows = computed(() => (hideUnreadable.value ? filtered.value.filter((r) => r.path !== null) : filtered.value))

watch(open, (v) => {
  if (v) {
    keyword.value = ""
    void refresh()
  }
})

watch([excludeProtected, ignoreSystem], () => {
  if (open.value) void refresh()
})

/**
 *
 * @param row
 */
function pick(row: ProcRow): void {
  if (row.is_protected) return
  emit("select", row)
  open.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="max-h-[80vh] max-w-xl gap-0 p-0">
      <DialogHeader class="border-b px-4 py-3">
        <DialogTitle class="text-sm">选择进程</DialogTitle>
        <DialogDescription class="text-xs"> 从当前运行的程序里挑，比手打路径可靠。受保护进程不会被处置，也不会出现在默认列表里。 </DialogDescription>
      </DialogHeader>

      <div class="border-b px-4 py-2">
        <div class="relative">
          <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2" />
          <Input v-model="keyword" placeholder="搜索应用名、进程名、窗口标题或路径" class="h-8 pr-12 pl-7 font-mono text-xs" />
          <button
            type="button"
            :aria-pressed="useRegex"
            title="按正则表达式匹配"
            class="absolute top-1/2 right-1.5 -translate-y-1/2 cursor-pointer rounded px-1.5 py-0.5 font-mono text-xs transition-colors duration-150"
            :class="useRegex ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-accent'"
            @click="useRegex = !useRegex"
          >
            .*
          </button>
        </div>
        <p v-if="regexError" class="text-destructive mt-1 text-xs">{{ regexError }}</p>
      </div>

      <div class="flex flex-wrap items-center gap-1.5 border-b px-4 py-2">
        <span class="text-muted-foreground text-xs">过滤</span>
        <button
          type="button"
          :aria-pressed="excludeProtected"
          class="flex cursor-pointer items-center gap-1 rounded-full border px-2 py-0.5 text-xs transition-colors duration-150"
          :class="excludeProtected ? 'border-primary/40 bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent'"
          @click="excludeProtected = !excludeProtected"
        >
          <Check v-if="excludeProtected" class="size-2.5" />
          排除受保护进程
        </button>
        <button
          type="button"
          :aria-pressed="ignoreSystem"
          class="flex cursor-pointer items-center gap-1 rounded-full border px-2 py-0.5 text-xs transition-colors duration-150"
          :class="ignoreSystem ? 'border-primary/40 bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent'"
          @click="ignoreSystem = !ignoreSystem"
        >
          <Check v-if="ignoreSystem" class="size-2.5" />
          忽略系统项
        </button>
        <button
          type="button"
          :aria-pressed="hideUnreadable"
          class="flex cursor-pointer items-center gap-1 rounded-full border px-2 py-0.5 text-xs transition-colors duration-150"
          :class="hideUnreadable ? 'border-primary/40 bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent'"
          @click="hideUnreadable = !hideUnreadable"
        >
          <Check v-if="hideUnreadable" class="size-2.5" />
          隐藏无路径项
        </button>
      </div>

      <div class="max-h-[46vh] overflow-y-auto">
        <div v-if="loading" class="flex flex-col gap-2 p-4">
          <Skeleton v-for="i in 6" :key="i" class="h-9 w-full" />
        </div>

        <div v-else-if="visibleRows.length === 0" class="text-muted-foreground p-8 text-center text-xs">
          没有匹配的进程。试试关掉上面的过滤开关，或检查搜索表达式。
        </div>

        <template v-else>
          <button
            v-for="row in visibleRows"
            :key="row.pid"
            type="button"
            :disabled="row.is_protected"
            class="flex w-full items-start gap-3 border-b px-4 py-2 text-left transition-colors last:border-0"
            :class="row.is_protected ? 'cursor-not-allowed opacity-60' : 'hover:bg-accent cursor-pointer'"
            @click="pick(row)"
          >
            <img v-if="iconOf(metaKey(row))" :src="iconOf(metaKey(row))" alt="" class="mt-0.5 size-4 shrink-0 rounded-sm" />
            <AppWindow v-else class="text-muted-foreground/60 mt-0.5 size-4 shrink-0" />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="truncate text-xs font-medium">
                  <template v-for="(seg, i) in segments(appLabel(row))" :key="i">
                    <mark v-if="seg.hit" class="bg-primary/25 text-foreground rounded-sm px-0.5">{{ seg.text }}</mark>
                    <template v-else>{{ seg.text }}</template>
                  </template>
                </span>
                <span v-if="row.instances > 1" class="text-muted-foreground shrink-0 text-xs"> ×{{ row.instances }} </span>
                <span v-if="row.is_protected" class="border-destructive/40 text-destructive shrink-0 rounded border px-1 text-[0.6875rem]">
                  受保护
                </span>
                <span v-else-if="row.is_system" class="text-muted-foreground shrink-0 rounded border px-1 text-[0.6875rem]"> 系统 </span>
              </div>
              <div v-if="row.window_title || appLabel(row) !== row.exe_name" class="text-muted-foreground truncate text-xs">
                <span v-if="appLabel(row) !== row.exe_name" class="font-mono">
                  {{ row.exe_name }}
                </span>
                <template v-if="row.window_title">
                  <span v-if="appLabel(row) !== row.exe_name"> · </span>
                  <template v-for="(seg, i) in segments(row.window_title)" :key="i">
                    <mark v-if="seg.hit" class="bg-primary/25 text-foreground rounded-sm px-0.5">{{ seg.text }}</mark>
                    <template v-else>{{ seg.text }}</template>
                  </template>
                </template>
              </div>
              <MiddleEllipsis :text="row.path ?? `PID ${row.pid}（无法读取路径）`" class="text-muted-foreground/70 text-xs" />
            </div>
          </button>
        </template>
      </div>
    </DialogContent>
  </Dialog>
</template>
