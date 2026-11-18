<script setup lang="ts">
import { AppWindow, GripVertical, Pencil, Trash2 } from "@lucide/vue"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Switch } from "@/components/ui/switch"
import { useAppMeta } from "@/composables/useAppMeta"
import { formatSecs } from "@/utils/format"

import type { Rule } from "@/types/models"

/**
 * 规则页的单条规则卡片。
 *
 * 拖拽属性由父层的 `useCardSort` 生成后原样透传，卡片本身不持有排序状态；
 * 开关 / 编辑 / 删除都只发事件，写操作统一留在页面里。
 */
withDefaults(
  defineProps<{
    rule: Rule
    cardAttrs: Record<string, unknown>
    handleAttrs: Record<string, unknown>
    dragging?: boolean
    over?: boolean
    highlight?: boolean
  }>(),
  { dragging: false, over: false, highlight: false }
)

const emit = defineEmits<{
  toggle: [enabled: boolean]
  edit: []
  remove: []
}>()

const { iconOf } = useAppMeta()

const ACTION_LABEL: Record<Rule["on_exceed"], string> = {
  // 无操作：额度用满后不做任何处置，也不会弹提醒
  notify_only: "无操作",
  graceful_close: "关闭前台",
  force_kill: "强制结束",
}
</script>

<template>
  <Card
    v-bind="cardAttrs"
    :id="`rule-${rule.id}`"
    :class="[
      highlight ? 'border-primary/60 bg-primary/5 ring-primary/40 ring-2' : '',
      over ? 'ring-primary/50 ring-2' : '',
      dragging ? 'opacity-60' : '',
    ]"
    class="scroll-mt-6 transition-[box-shadow,border-color,background-color] duration-500"
  >
    <CardContent class="flex items-center gap-3 py-3">
      <!-- 拖动手柄：整卡只有这里可拖，避免拖开关时误触 -->
      <span
        v-bind="handleAttrs"
        class="text-muted-foreground/50 hover:text-muted-foreground shrink-0 cursor-grab transition-colors active:cursor-grabbing"
      >
        <GripVertical class="size-4" />
      </span>

      <Switch :model-value="rule.enabled" @update:model-value="(v: boolean) => emit('toggle', v)" />

      <img v-if="iconOf(rule.pattern)" :src="iconOf(rule.pattern)" alt="" draggable="false" class="size-8 shrink-0 rounded-sm" />
      <AppWindow v-else class="text-muted-foreground/60 size-8 shrink-0" />

      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate text-sm font-medium">{{ rule.display_name }}</span>
          <Badge
            class="border-transparent text-xs"
            :class="
              rule.kind === 'focus'
                ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400'
                : 'bg-amber-500/15 text-amber-700 dark:text-amber-400'
            "
          >
            {{ rule.kind === "focus" ? "专注" : "摸鱼" }}
          </Badge>
          <Badge variant="secondary" class="text-xs">
            {{ rule.mode === "running" ? "存在即计时" : "仅前台" }}
          </Badge>
          <Badge variant="outline" class="text-xs">
            {{ ACTION_LABEL[rule.on_exceed] }}
          </Badge>
          <Badge v-if="rule.allow_background && rule.on_exceed !== 'notify_only'" variant="outline" class="text-xs"> 保留后台 </Badge>
        </div>
        <div class="text-muted-foreground truncate font-mono text-xs">{{ rule.match_kind === "full_path" ? "" : "[按名称] " }}{{ rule.pattern }}</div>
      </div>

      <div class="hidden shrink-0 text-right sm:block">
        <div class="font-mono text-xs tabular-nums">{{ Math.round(rule.daily_limit_secs / 60) }} 分钟</div>
        <div class="text-muted-foreground text-xs">
          {{
            rule.kind === "focus" && rule.rest_remind_secs > 0
              ? `每 ${Math.round(rule.rest_remind_secs / 60)} 分钟休息`
              : `宽限 ${formatSecs(rule.grace_secs)}`
          }}
        </div>
      </div>

      <div class="flex shrink-0 gap-1">
        <Button variant="ghost" size="icon-sm" @click="emit('edit')">
          <Pencil />
        </Button>
        <Button variant="ghost" size="icon-sm" @click="emit('remove')">
          <Trash2 class="text-destructive" />
        </Button>
      </div>
    </CardContent>
  </Card>
</template>
