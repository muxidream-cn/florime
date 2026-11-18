<script setup lang="ts">
import { ChevronDown, ExternalLink } from "@lucide/vue"
import { cn } from "cn"
import { computed, ref } from "vue"

import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Separator } from "@/components/ui/separator"
import { openUrl } from "@/lib/appInfo"

type Library = {
  name: string
  license: string
  url: string
  note?: string
}

type LibraryGroup = {
  title: string
  entries: Library[]
}

/**
 * 前端运行时 + Tauri 2 依赖清单，以本项目 package.json / Cargo.toml 实际安装为准。
 * 许可证取自各包声明的 license 字段，官网链接缺失处留空（按钮置灰）。
 */
const groups: LibraryGroup[] = [
  {
    title: "框架与状态",
    entries: [
      { name: "Vue", license: "MIT", url: "https://vuejs.org/", note: "UI 框架" },
      { name: "Vue Router", license: "MIT", url: "https://router.vuejs.org/", note: "路由" },
      { name: "Pinia", license: "MIT", url: "https://pinia.vuejs.org/", note: "状态管理" },
      { name: "dayjs", license: "MIT", url: "https://day.js.org/", note: "日期处理" },
    ],
  },
  {
    title: "UI 组件与样式",
    entries: [
      { name: "reka-ui", license: "MIT", url: "https://reka-ui.com/", note: "无样式组件基础" },
      { name: "shadcn-vue", license: "MIT", url: "https://www.shadcn-vue.com/", note: "组件集合" },
      { name: "@lucide/vue", license: "ISC", url: "https://lucide.dev/", note: "图标" },
      {
        name: "class-variance-authority",
        license: "Apache-2.0",
        url: "https://github.com/joe-bell/cva",
        note: "组件变体样式",
      },
      { name: "clsx", license: "MIT", url: "https://github.com/lukeed/clsx", note: "类名工具" },
      {
        name: "tailwind-merge",
        license: "MIT",
        url: "https://github.com/dcastil/tailwind-merge",
        note: "类名合并",
      },
      { name: "tw-animate-css", license: "MIT", url: "https://github.com/Wombosvideo/tw-animate-css", note: "动画" },
      { name: "cn", license: "MIT", url: "https://github.com/shadcn-ui/cn", note: "类名合并" },
    ],
  },
  {
    title: "工具",
    entries: [
      { name: "@vueuse/core", license: "MIT", url: "https://github.com/vueuse/vueuse", note: "组合式工具集" },
      { name: "vue-sonner", license: "MIT", url: "https://github.com/xiaoluoboding/vue-sonner", note: "消息提示" },
    ],
  },
  {
    title: "Tauri 2 平台 API（JS 侧）",
    entries: [
      { name: "@tauri-apps/api", license: "MIT", url: "https://github.com/tauri-apps/tauri" },
      {
        name: "@tauri-apps/plugin-dialog",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
      },
    ],
  },
  {
    title: "Tauri 2（Rust 侧）",
    entries: [
      {
        name: "tauri",
        license: "Apache-2.0 OR MIT",
        url: "https://github.com/tauri-apps/tauri",
        note: "tray-icon / image-png",
      },
      {
        name: "tauri-plugin-opener",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
      },
      {
        name: "tauri-plugin-log",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
      },
      {
        name: "tauri-plugin-dialog",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
      },
      {
        name: "tauri-plugin-notification",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
        note: "仅 Windows",
      },
      {
        name: "tauri-plugin-autostart",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
        note: "仅 Windows",
      },
      {
        name: "tauri-plugin-single-instance",
        license: "MIT OR Apache-2.0",
        url: "https://github.com/tauri-apps/plugins-workspace",
        note: "仅 Windows",
      },
    ],
  },
  {
    title: "Rust 依赖",
    entries: [
      { name: "serde", license: "MIT OR Apache-2.0", url: "https://github.com/serde-rs/serde", note: "序列化框架" },
      { name: "serde_json", license: "MIT OR Apache-2.0", url: "https://github.com/serde-rs/json", note: "JSON 序列化" },
      { name: "chrono", license: "MIT OR Apache-2.0", url: "https://github.com/chronotope/chrono", note: "日期时间" },
      { name: "rusqlite", license: "MIT", url: "https://github.com/rusqlite/rusqlite", note: "SQLite 绑定" },
      { name: "uuid", license: "Apache-2.0 OR MIT", url: "https://github.com/uuid-rs/uuid", note: "UUID 生成" },
      { name: "thiserror", license: "MIT OR Apache-2.0", url: "https://github.com/dtolnay/thiserror", note: "错误派生宏" },
      { name: "log", license: "MIT OR Apache-2.0", url: "https://github.com/rust-lang/log", note: "日志门面" },
      { name: "png", license: "MIT OR Apache-2.0", url: "https://github.com/image-rs/image-png", note: "PNG 编解码" },
      { name: "base64", license: "MIT OR Apache-2.0", url: "https://github.com/marshallpierce/rust-base64", note: "Base64 编解码" },
      { name: "windows", license: "MIT OR Apache-2.0", url: "https://github.com/microsoft/windows-rs", note: "Win32 API 绑定（仅 Windows）" },
      { name: "tauri-build", license: "MIT OR Apache-2.0", url: "https://github.com/tauri-apps/tauri", note: "构建依赖" },
    ],
  },
]

const isOpen = ref(false)
const total = computed(() => groups.reduce((sum, group) => sum + group.entries.length, 0))
</script>

<template>
  <Collapsible v-model:open="isOpen">
    <CollapsibleTrigger as-child>
      <Button variant="outline" size="sm" class="h-10 w-full justify-between">
        <span>第三方开源组件（{{ total }}）</span>
        <ChevronDown :class="cn('transition-transform', isOpen && 'rotate-180')" />
      </Button>
    </CollapsibleTrigger>

    <CollapsibleContent class="mt-3 flex max-h-80 flex-col gap-4 overflow-y-auto overscroll-contain pr-1 select-text">
      <p class="text-muted-foreground text-xs">
        本项目基于 MIT 许可发布。项目包含或使用了以下第三方软件，各组件仍按其各自的许可证授权；MIT
        许可证的条款不适用于这些第三方组件，具体许可证文本以各项目仓库或官方许可页面为准。
      </p>

      <div v-for="group in groups" :key="group.title" class="flex flex-col gap-1.5">
        <h4 class="text-sm font-semibold">{{ group.title }}</h4>
        <div class="text-muted-foreground flex items-center gap-3 px-2 text-[11px]">
          <span class="min-w-0 flex-1">组件</span>
          <span class="w-32 shrink-0">许可证</span>
          <span class="w-8 shrink-0" />
        </div>
        <div class="flex flex-col">
          <div v-for="entry in group.entries" :key="entry.name" class="border-border/60 flex items-center gap-3 border-b py-1.5 last:border-b-0">
            <div class="min-w-0 flex-1">
              <div class="text-xs font-medium">{{ entry.name }}</div>
              <div v-if="entry.note" class="text-muted-foreground text-[11px]">{{ entry.note }}</div>
            </div>
            <div class="text-muted-foreground w-32 shrink-0 text-xs">{{ entry.license }}</div>
            <Button
              variant="ghost"
              size="icon-sm"
              class="shrink-0"
              :disabled="!entry.url"
              :title="entry.url || '暂无官网链接'"
              @click="openUrl(entry.url)"
            >
              <ExternalLink />
            </Button>
          </div>
        </div>
      </div>

      <Separator />
      <p class="text-muted-foreground text-[11px]">
        许可证信息以已安装依赖包 package.json / Cargo.toml 的 license 字段为基础；本清单覆盖前端运行时依赖与 Tauri 2
        平台依赖，不含开发/构建工具与服务端依赖。
      </p>
    </CollapsibleContent>
  </Collapsible>
</template>
