<script setup lang="ts">
import { Coffee, Target } from "@lucide/vue"

import type { RuleKind } from "@/types/models"

/**
 * 规则模式的分段控件：整宽两格，选中项用品牌色填充并轻微上浮。
 *
 * 专注用青绿、摸鱼用琥珀，与提醒区块、列表徽章保持同一套强调色。
 */
const kind = defineModel<RuleKind>({ required: true })

const OPTIONS = [
  {
    value: "focus" as RuleKind,
    label: "专注",
    desc: "按累计时长定时提醒休息",
    icon: Target,
    active: "bg-emerald-600 text-white shadow-lg shadow-emerald-600/25",
  },
  {
    value: "slack" as RuleKind,
    label: "摸鱼",
    desc: "只管额度，超限再处置",
    icon: Coffee,
    active: "bg-amber-500 text-white shadow-lg shadow-amber-500/25",
  },
] as const
</script>

<template>
  <div class="bg-muted/40 grid grid-cols-2 gap-1.5 rounded-xl border p-1.5" role="tablist">
    <button
      v-for="opt in OPTIONS"
      :key="opt.value"
      type="button"
      role="tab"
      :aria-selected="kind === opt.value"
      class="flex items-start gap-2.5 rounded-lg px-3 py-2.5 text-left transition-all duration-200"
      :class="kind === opt.value ? `${opt.active} -translate-y-px` : 'text-muted-foreground hover:bg-background/70'"
      @click="kind = opt.value"
    >
      <component :is="opt.icon" class="mt-0.5 size-4 shrink-0" />
      <span class="min-w-0">
        <span class="block text-xs font-semibold">{{ opt.label }}</span>
        <span class="block text-[0.6875rem] leading-snug opacity-80">{{ opt.desc }}</span>
      </span>
    </button>
  </div>
</template>
