<script setup lang="ts">
import { LoaderCircle, RefreshCcw } from "@lucide/vue"
import { computed } from "vue"
import { toast } from "vue-sonner"

import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { useUpdater } from "@/composables/useUpdater"
import { APP_VERSION } from "@/lib/appInfo"

const updater = useUpdater()

const autoCheck = computed({
  get: () => updater.autoCheck.value,
  set: (value: boolean) => updater.setAutoCheck(value),
})

/**
 *
 */
async function checkUpdate(): Promise<void> {
  try {
    const hasUpdate = await updater.checkForUpdate()
    if (!hasUpdate) toast.success("当前已是最新版本")
  } catch (e) {
    toast.error("检查更新失败", { description: e instanceof Error ? e.message : String(e) })
  }
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex items-center justify-between gap-4">
      <div>
        <div class="text-xs">启动时检查更新</div>
        <div class="text-muted-foreground text-xs">本地偏好，启动时自动检查一次</div>
      </div>
      <Switch v-model="autoCheck" />
    </div>

    <Separator />

    <div class="flex items-center justify-between gap-4">
      <div>
        <div class="text-xs">当前版本</div>
        <div class="text-muted-foreground text-xs">V{{ APP_VERSION }}</div>
      </div>
      <Button variant="outline" size="sm" :disabled="updater.checking.value" @click="checkUpdate">
        <LoaderCircle v-if="updater.checking.value" class="animate-spin" data-icon="inline-start" />
        <RefreshCcw v-else data-icon="inline-start" />
        检查更新
      </Button>
    </div>
  </div>
</template>
