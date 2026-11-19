<script setup lang="ts">
import { Loader } from "@lucide/vue"
import dayjs from "dayjs"
import { computed, shallowRef } from "vue"
import { toast } from "vue-sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Progress } from "@/components/ui/progress"
import { Separator } from "@/components/ui/separator"
import { useUpdater } from "@/composables/useUpdater"
import { formatByte } from "@/lib/formatByte"

const updater = useUpdater()

const open = computed({
  get: () => updater.dialogOpen.value,
  set: (value: boolean) => {
    if (!value) updater.cancel()
  },
})

const info = computed(() => updater.updateInfo.value)
const notes = computed(() => info.value.notes.replace(/\\n/g, "\n"))
const publishedAt = computed(() => (info.value.pubDate ? dayjs(info.value.pubDate).format("YY-MM-DD HH:mm:ss") : "未知"))
const packageSize = computed(() => (info.value.size > 0 ? formatByte(info.value.size, "MB") : "未知"))
const sizeLabel = computed(() => `${formatByte(updater.progress.value.downloaded)} / ${formatByte(updater.progress.value.total)}`)

const percent = computed(() => {
  const { total, downloaded, type } = updater.progress.value
  if (type === "finished") return 100
  if (total <= 0 || downloaded <= 0) return 0
  return Math.min(100, Math.round((downloaded * 100) / total))
})

const downloading = computed(() => updater.status.value === "downloading")
const downloaded = computed(() => updater.status.value === "ready")
const forceUpdate = computed(() => info.value.forceUpdate)

const localBusy = shallowRef(false)

/**
 *
 */
async function handleUpdate(): Promise<void> {
  localBusy.value = true
  try {
    await updater.downloadAndInstall()
    toast.success("更新已下载并安装")
  } catch (e) {
    toast.error("更新失败", { description: e instanceof Error ? e.message : String(e) })
  } finally {
    localBusy.value = false
  }
}

/**
 *
 */
async function handleInstall(): Promise<void> {
  localBusy.value = true
  try {
    await updater.installApp()
  } catch (e) {
    toast.error("安装失败", { description: e instanceof Error ? e.message : String(e) })
  } finally {
    localBusy.value = false
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent :show-close-button="false" class="max-w-100 text-sm" @pointer-down-outside.prevent @escape-key-down.prevent>
      <DialogHeader class="text-center">
        <DialogTitle class="text-lg">发现新版本</DialogTitle>
        <Separator />
      </DialogHeader>

      <DialogDescription class="text-start">
        <div class="flex justify-between gap-2">
          <div><span class="text-sm">最新版本：</span>{{ info.version }}</div>
          <div><span class="text-sm">当前版本：</span>{{ info.currentVersion }}</div>
        </div>
        <div><span class="text-sm">发布时间：</span>{{ publishedAt }}</div>
        <div><span class="text-sm">包体大小：</span>{{ packageSize }}</div>
      </DialogDescription>

      <div class="grid gap-2">
        <div v-if="downloading" class="flex flex-nowrap items-center justify-between gap-2">
          <span class="shrink-0 text-sm">下载进度：</span>
          <Progress :model-value="percent" class="m-2 h-2 min-w-0 flex-1" />
          <span class="shrink-0 font-bold tabular-nums">{{ sizeLabel }}</span>
        </div>

        <div class="max-h-20 overflow-y-auto">
          <DialogDescription class="text-sm">更新说明：</DialogDescription>
          <DialogDescription v-if="forceUpdate" class="text-amber-300">* 本次更新为强制更新</DialogDescription>
          <div class="text-muted-foreground text-xs whitespace-pre-wrap">{{ notes }}</div>
        </div>
      </div>

      <DialogDescription v-if="updater.downloadFailureCount.value > 0" class="text-destructive" aria-live="polite">
        下载失败 {{ updater.downloadFailureCount.value }} 次，可重试。
      </DialogDescription>

      <DialogFooter class="flex-wrap gap-2">
        <template v-if="downloaded">
          <Button :disabled="localBusy" @click="handleInstall">
            <Loader v-if="localBusy" class="animate-spin" />
            安装并重启
          </Button>
        </template>
        <template v-else>
          <Button v-if="!forceUpdate" variant="outline" :disabled="localBusy || updater.isBusy.value" @click="updater.cancel()"> 取消更新 </Button>
          <Button :disabled="localBusy || updater.isBusy.value" @click="handleUpdate">
            <Loader v-if="localBusy || downloading" class="animate-spin" />
            下载并安装
          </Button>
        </template>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
