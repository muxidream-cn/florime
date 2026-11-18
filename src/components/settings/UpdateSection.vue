<script setup lang="ts">
import { RefreshCcw } from "@lucide/vue"

import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { APP_VERSION } from "@/lib/appInfo"
</script>

<template>
  <div class="flex flex-col gap-3">
    <!-- 更新能力尚未接入，全部置灰，仅占位 UI -->
    <div class="flex items-center justify-between gap-4">
      <div>
        <div class="text-xs">启动时检查更新</div>
        <div class="text-muted-foreground text-xs">暂未实现</div>
      </div>
      <Switch disabled />
    </div>

    <Separator />

    <div class="flex items-center justify-between gap-4">
      <div>
        <div class="text-xs">当前版本</div>
        <div class="text-muted-foreground text-xs">V{{ APP_VERSION }}</div>
      </div>
      <Button variant="outline" size="sm" disabled>
        <RefreshCcw data-icon="inline-start" />
        检查更新
      </Button>
    </div>
  </div>
</template>
