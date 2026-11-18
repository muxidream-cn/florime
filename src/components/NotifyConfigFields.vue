<script setup lang="ts">
import { computed, watch } from "vue"

import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

import type { NotifyConfig } from "@/types/models"

/**
 * 提醒呈现配置：文案 + 三级开关（全屏 → 不可关闭 → 屏蔽快捷键）+ 自动消失。
 *
 * 两个提醒（休息 / 超限）共用这一份，父组件只负责给一个 reactive 的 `NotifyConfig`。
 * 开关之间存在依赖：关闭上游会连带清空下游，这里用 watch 兜住，
 * 保证 UI 上永远不会出现「没全屏却不可关闭」这种矛盾组合。
 */
const props = withDefaults(
  defineProps<{
    /**
     * 该提醒的展示时长由 auto_close_secs 控制，与「不可关闭」无关。
     *
     * 休息提醒到点必收（后端无条件用 auto_close_secs 设截止时间），所以那边要
     * 常开可编辑；超限提醒只有「不可关闭」时才用得上，此时置灰并禁用输入。
     */
    autoCloseAlways?: boolean
  }>(),
  { autoCloseAlways: false }
)
const model = defineModel<NotifyConfig>({ required: true })

/** 「持续时间」当前是否有意义：决定置灰与能否编辑，两者必须由同一条件驱动 */
const autoCloseActive = computed(() => props.autoCloseAlways || model.value.block_close)

watch(
  () => [model.value.fullscreen, model.value.block_close] as const,
  ([fullscreen, blockClose]) => {
    if (!fullscreen) {
      model.value.block_close = false
      model.value.block_shortcuts = false
      return
    }
    if (!blockClose) model.value.block_shortcuts = false
  }
)

/**
 *
 * @param value
 */
function onMessage(value: string | number): void {
  model.value.message = String(value)
}

/**
 *
 * @param value
 */
function onAutoClose(value: string | number): void {
  const n = Number(value)
  if (Number.isFinite(n)) model.value.auto_close_secs = Math.round(n)
}

/** 失焦时再夹紧数值，避免边输入边被改写（输入 10 时先变成 5 的尴尬） */
function clampAutoClose(): void {
  model.value.auto_close_secs = Math.min(300, Math.max(5, model.value.auto_close_secs || 30))
}
</script>

<template>
  <div class="flex flex-col gap-2.5">
    <div class="flex flex-col gap-1.5">
      <div class="text-xs font-medium">提醒文案</div>
      <textarea
        :value="model.message"
        rows="2"
        placeholder="留空用默认文案，例如「该起来活动一下了」"
        class="border-input placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 w-full resize-none rounded-md border bg-transparent px-2.5 py-1.5 text-xs leading-relaxed shadow-xs transition-[color,box-shadow] outline-none focus-visible:ring-3"
        @input="onMessage(($event.target as HTMLTextAreaElement).value)"
      />
    </div>

    <div class="bg-muted/30 flex items-center justify-between gap-3 rounded-md border px-2.5 py-2">
      <div class="min-w-0">
        <div class="text-xs font-medium">全屏提醒</div>
        <div class="text-muted-foreground text-[0.6875rem]">覆盖当前显示器、置顶并抢焦点</div>
      </div>
      <Switch :model-value="model.fullscreen" @update:model-value="(v: boolean) => (model.fullscreen = v)" />
    </div>

    <div
      class="flex items-center justify-between gap-3 rounded-md border border-dashed px-2.5 py-2 pl-4 transition-opacity"
      :class="model.fullscreen ? 'opacity-100' : 'opacity-50'"
    >
      <div class="min-w-0">
        <div class="text-xs font-medium">不可关闭</div>
        <div class="text-muted-foreground text-[0.6875rem]">
          {{ model.fullscreen ? "全屏提醒弹窗不可关闭，只能等倒计时结束" : "需要先开启全屏提醒" }}
        </div>
      </div>
      <Switch :model-value="model.block_close" :disabled="!model.fullscreen" @update:model-value="(v: boolean) => (model.block_close = v)" />
    </div>

    <div
      class="flex items-center justify-between gap-3 rounded-md border border-dashed px-2.5 py-2 pl-7 transition-opacity"
      :class="model.fullscreen && model.block_close ? 'opacity-100' : 'opacity-50'"
    >
      <div class="min-w-0">
        <div class="text-xs font-medium">屏蔽切出快捷键</div>
        <div class="text-muted-foreground text-[0.6875rem]">
          {{
            model.fullscreen && model.block_close
              ? "拦截 Win、Alt+Tab、Alt+F4、Ctrl+Esc；Ctrl+Alt+Del 等系统组合无法拦截"
              : "需要先开启「全屏 + 不可关闭」"
          }}
        </div>
      </div>
      <Switch
        :model-value="model.block_shortcuts"
        :disabled="!model.fullscreen || !model.block_close"
        @update:model-value="(v: boolean) => (model.block_shortcuts = v)"
      />
    </div>

    <div class="flex items-center justify-between gap-3 pl-7 transition-opacity" :class="autoCloseActive ? 'opacity-100' : 'opacity-60'">
      <div class="min-w-0">
        <div class="text-xs font-medium">持续时间</div>
        <div class="text-muted-foreground text-[0.6875rem]">
          {{ autoCloseActive ? "5–300 秒，到点自动收窗" : "仅「不可关闭」的提醒才需要设置" }}
        </div>
      </div>
      <div class="flex items-center gap-1.5">
        <Input
          :model-value="model.auto_close_secs"
          type="number"
          min="5"
          max="300"
          class="h-8 w-20 text-center text-xs tabular-nums"
          :disabled="!autoCloseActive"
          @update:model-value="onAutoClose"
          @blur="clampAutoClose"
        />
        <span class="text-muted-foreground text-xs">秒</span>
      </div>
    </div>
  </div>
</template>
