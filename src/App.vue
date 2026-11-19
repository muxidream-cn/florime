<script setup lang="ts">
import { onMounted } from "vue"

import { Toaster } from "@/components/ui/sonner"
import UpdateDialog from "@/components/UpdateDialog.vue"
import { useUpdater } from "@/composables/useUpdater"

const updater = useUpdater()

let autoCheckedThisSession = false

onMounted(() => {
  if (!updater.autoCheck.value || autoCheckedThisSession) return
  autoCheckedThisSession = true
  // 延后检查，避开首窗渲染与配置加载的高峰
  window.setTimeout(() => {
    void updater.checkForUpdate(true).catch(() => {
      // 静默失败：启动时不打扰用户，手动检查仍会给出明确提示
    })
  }, 3000)
})
</script>

<template>
  <RouterView />
  <UpdateDialog />
  <Toaster position="bottom-right" :visible-toasts="3" />
</template>
