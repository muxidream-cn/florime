<script setup lang="ts">
import { ExternalLink } from "@lucide/vue"

import ThirdPartyNotices from "@/components/settings/ThirdPartyNotices.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { APP_COPYRIGHT, APP_DESCRIPTION, APP_LICENSE, APP_LINKS, APP_NAME, APP_VERSION, openUrl } from "@/lib/appInfo"
</script>

<template>
  <div class="flex flex-col gap-4 text-left">
    <!-- 应用信息 -->
    <div class="flex items-center gap-4">
      <div class="bg-primary text-primary-foreground grid size-14 shrink-0 place-items-center rounded-lg">
        <span class="text-2xl font-bold">锁</span>
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <div class="flex items-center gap-2">
          <span class="text-base font-medium">{{ APP_NAME }}</span>
          <Badge variant="secondary">V{{ APP_VERSION }}</Badge>
        </div>
        <p class="text-muted-foreground text-xs">{{ APP_DESCRIPTION }}</p>
        <p class="text-muted-foreground text-xs">本软件是基于 {{ APP_LICENSE }} 许可证的免费开源软件，第三方组件仍按其各自的许可证授权。</p>
      </div>
    </div>

    <Separator />

    <!-- 外部链接：项目地址 / 网站地址 / 隐私协议，URL 待补充 -->
    <template v-for="link in APP_LINKS" :key="link.label">
      <div class="flex items-center justify-between gap-6">
        <span class="text-sm">{{ link.label }}</span>
        <Button variant="outline" size="sm" :disabled="!link.url" @click="openUrl(link.url)">
          <ExternalLink data-icon="inline-start" />
          浏览器打开
        </Button>
      </div>
      <Separator />
    </template>

    <!-- 第三方开源组件声明：折叠子区块 -->
    <ThirdPartyNotices />

    <p class="text-muted-foreground text-center text-xs">{{ APP_COPYRIGHT }}</p>
  </div>
</template>
