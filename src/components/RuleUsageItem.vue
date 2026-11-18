<script setup lang="ts">
import { AppWindow, ExternalLink, GripVertical, Pause, Play, PowerOff, ShieldAlert, Timer } from "@lucide/vue"

import { revealRulePath } from "@/api/commands"
import MiddleEllipsis from "@/components/MiddleEllipsis.vue"
import RuleTimeline from "@/components/RuleTimeline.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { useAppMeta } from "@/composables/useAppMeta"

import type { Rule, RuleKind, RuleUsage } from "@/types/models"
import type { RuleTimelineItem, UsageTone } from "@/utils/usage"

interface BadgeInfo {
  label: string
  variant: "secondary" | "destructive" | "outline"
  class: string
}

/**
 * 概览页的单条规则用量卡片。
 *
 * 拖拽属性由父层的 `useCardSort` 提供并原样透传，卡片本身不持有排序状态，
 * 这样规则页与概览页可以共用同一份顺序。倒计时时间线也由父层按快照算好传入，
 * 卡片只负责排版，不再自己起定时器。
 */
const props = withDefaults(
  defineProps<{
    usage: RuleUsage
    rule?: Rule
    /** 规则模式：专注 / 摸鱼 */
    kind: RuleKind
    /** 规则三态：启用 / 暂停 / 关闭 */
    ruleStatus?: "enabled" | "paused" | "closed"
    /** 规则配置了全屏提醒 */
    hasFullscreen?: boolean
    tone: UsageTone
    badge: BadgeInfo | null
    /** 超限处置方式的中文短标签 */
    actionLabel: string
    timelines: RuleTimelineItem[]
    cardAttrs: Record<string, unknown>
    handleAttrs: Record<string, unknown>
    dragging?: boolean
    over?: boolean
  }>(),
  {
    rule: undefined,
    ruleStatus: "enabled",
    hasFullscreen: false,
    dragging: false,
    over: false,
  }
)

const emit = defineEmits<{
  togglePaused: [paused: boolean]
  jump: []
}>()

const { iconOf } = useAppMeta()

/**
 *
 */
async function openPatternDir(): Promise<void> {
  if (!props.rule?.pattern) return
  try {
    await revealRulePath(props.rule.pattern)
  } catch (e) {
    console.error("打开程序目录失败", e)
  }
}
</script>

<template>
  <Card
    v-bind="cardAttrs"
    class="flex h-full min-w-0 flex-col overflow-hidden"
    :class="[
      ruleStatus === 'closed' ? 'bg-muted/30 opacity-55 grayscale-[0.6]' : '',
      usage.state === 'enforced' && ruleStatus !== 'closed'
        ? 'border-destructive/50'
        : usage.state === 'cleanup' && ruleStatus !== 'closed'
          ? 'border-amber-500/50'
          : '',
      over ? 'ring-primary/50 ring-2' : '',
      dragging ? 'opacity-60' : '',
    ]"
  >
    <CardHeader class="relative w-full min-w-0 pb-2">
      <!-- 拖动手柄：整卡只有这里可拖 -->
      <span
        v-bind="handleAttrs"
        class="text-muted-foreground/50 hover:text-muted-foreground absolute top-1 left-1 shrink-0 cursor-grab transition-colors active:cursor-grabbing"
      >
        <GripVertical class="size-4" />
      </span>
      <div class="flex min-w-0 items-center justify-between gap-2">
        <div class="flex min-w-0 items-center gap-1.5">
          <img v-if="iconOf(rule?.pattern)" :src="iconOf(rule?.pattern)" alt="" draggable="false" class="size-6 shrink-0 rounded-sm" />
          <AppWindow v-else class="text-muted-foreground/60 size-6 shrink-0" />
          <CardTitle class="truncate text-sm">{{ usage.display_name }}</CardTitle>
        </div>
        <div class="flex shrink-0 items-center gap-1">
          <Badge v-if="badge && ruleStatus !== 'closed'" :variant="badge.variant" class="text-xs whitespace-nowrap" :class="badge.class">
            {{ badge.label }}
          </Badge>
          <Button
            v-if="ruleStatus === 'enabled'"
            size="icon"
            variant="ghost"
            class="text-muted-foreground hover:text-foreground size-6 cursor-pointer"
            title="暂停这条规则"
            @click="emit('togglePaused', true)"
          >
            <Pause class="size-3.5" />
          </Button>
          <Button
            v-else-if="ruleStatus === 'paused'"
            size="icon"
            variant="ghost"
            class="size-6 cursor-pointer text-emerald-600 hover:text-emerald-500"
            title="开始计时"
            @click="emit('togglePaused', false)"
          >
            <Play class="size-3.5" />
          </Button>
          <span v-else class="grid size-6 place-items-center" title="规则已关闭">
            <PowerOff class="text-muted-foreground/70 size-3.5" />
          </span>
          <Button
            size="icon"
            variant="ghost"
            class="text-muted-foreground hover:text-foreground size-6 cursor-pointer"
            title="跳转到规则配置"
            @click="emit('jump')"
          >
            <ExternalLink class="size-3.5" />
          </Button>
        </div>
      </div>
      <CardDescription class="mt-1 w-full min-w-0 overflow-hidden font-mono text-xs">
        <button
          type="button"
          class="hover:text-primary block w-full cursor-pointer text-left transition-colors hover:underline"
          title="打开程序所在目录"
          @click="openPatternDir"
        >
          <MiddleEllipsis class="w-full" :text="rule?.pattern ?? ''" :tail-chars="16" />
        </button>
      </CardDescription>
    </CardHeader>
    <CardContent class="flex flex-1 flex-col gap-2.5">
      <!-- 倒计时时间线：休息 / 超限，最多两条，随快照每秒刷新 -->
      <div v-if="timelines.length > 0" class="flex flex-col gap-2">
        <RuleTimeline v-for="item in timelines" :key="item.id" v-bind="item" />
      </div>

      <div class="mt-auto flex flex-col gap-2 pt-1">
        <div class="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem]">
          <Badge
            class="shrink-0 border-transparent px-1.5 text-[0.6875rem]"
            :class="
              kind === 'focus' ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400' : 'bg-amber-500/15 text-amber-700 dark:text-amber-400'
            "
          >
            {{ kind === "focus" ? "专注" : "摸鱼" }}
          </Badge>
          <span class="flex items-center gap-1 whitespace-nowrap">
            <Timer class="size-3 shrink-0" />
            {{ rule?.mode === "running" ? "存在即计时" : "仅前台" }}
          </span>
          <Badge variant="outline" class="gap-1 px-1.5 text-[0.6875rem] font-normal">
            <ShieldAlert class="size-2.5 shrink-0" />
            {{ actionLabel }}
          </Badge>
          <Badge v-if="hasFullscreen" variant="outline" class="px-1.5 text-[0.6875rem] whitespace-nowrap"> 全屏提醒 </Badge>
        </div>
        <div
          v-if="usage.state === 'cleanup'"
          class="rounded-md border border-amber-500/30 bg-amber-500/5 px-2 py-1 text-xs text-amber-700 dark:text-amber-300"
        >
          后台仍有 {{ usage.pending_procs }} 个进程，请自行关闭；每轮复检都会重弹提醒
        </div>
      </div>
    </CardContent>
  </Card>
</template>
