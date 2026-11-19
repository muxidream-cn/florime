<script setup lang="ts">
import { Download, LoaderCircle, RefreshCw } from "@lucide/vue"
import { computed } from "vue"
import { toast } from "vue-sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Progress } from "@/components/ui/progress"
import { useUpdater } from "@/composables/useUpdater"

const updater = useUpdater()

const open = computed({
  get: () => updater.dialogOpen.value,
  set: (value: boolean) => {
    if (!value) updater.dismissDialog()
  },
})

const percent = computed(() => {
  const { total, downloaded, type } = updater.progress.value
  if (type === "finished") return 100
  if (total <= 0) return 0
  return Math.min(100, Math.round((downloaded / total) * 100))
})

const showProgress = computed(() => {
  const status = updater.status.value
  return status === "downloading" || status === "ready"
})

/**
 *
 */
async function handleDownload(): Promise<void> {
  try {
    await updater.updateApp()
  } catch (e) {
    toast.error("下载更新失败", { description: e instanceof Error ? e.message : String(e) })
  }
}

/**
 *
 */
async function handleInstall(): Promise<void> {
  try {
    await updater.installApp()
  } catch (e) {
    toast.error("安装更新失败", { description: e instanceof Error ? e.message : String(e) })
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <RefreshCw v-if="updater.status.value === 'available'" class="size-4" />
          <Download v-else class="size-4" />
          发现新版本 V{{ updater.newVersion.value }}
        </DialogTitle>
        <DialogDescription>
          {{ updater.newVersionBody.value || "有新版本可供下载，更新后应用将重新启动。" }}
        </DialogDescription>
      </DialogHeader>

      <div v-if="showProgress" class="flex flex-col gap-2">
        <div class="text-muted-foreground flex justify-between text-xs">
          <span>{{ updater.status.value === "ready" ? "下载完成" : "正在下载更新" }}</span>
          <span>{{ percent }}%</span>
        </div>
        <Progress :model-value="percent" />
      </div>

      <div v-if="updater.error.value" class="text-destructive text-xs">{{ updater.error.value }}</div>

      <DialogFooter>
        <Button variant="outline" :disabled="showProgress" @click="updater.dismissDialog()">稍后</Button>
        <Button v-if="updater.status.value === 'available'" :disabled="updater.isBusy.value" @click="handleDownload">
          <Download data-icon="inline-start" />
          下载更新
        </Button>
        <Button v-else-if="updater.status.value === 'ready'" @click="handleInstall">
          <RefreshCw data-icon="inline-start" />
          安装并重启
        </Button>
        <Button v-else-if="updater.status.value === 'downloading'" disabled>
          <LoaderCircle class="animate-spin" data-icon="inline-start" />
          下载中
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
