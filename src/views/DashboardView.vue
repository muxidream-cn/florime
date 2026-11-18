<script setup lang="ts">
import { AppWindow, Filter, ListChecks } from "@lucide/vue"
import { ToggleGroupItem, ToggleGroupRoot } from "reka-ui"
import { computed, onMounted, ref, watch } from "vue"
import { useRouter } from "vue-router"

import DualUsageRing from "@/components/DualUsageRing.vue"
import MiddleEllipsis from "@/components/MiddleEllipsis.vue"
import RuleUsageItem from "@/components/RuleUsageItem.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { useAppMeta } from "@/composables/useAppMeta"
import { useCardSort } from "@/composables/useCardSort"
import { useMonitorStore } from "@/stores/monitor"
import { useRulesStore } from "@/stores/rules"
import { formatDuration } from "@/utils/format"
import { usageTone, type RuleTimelineItem } from "@/utils/usage"

import type { Rule, RuleUsage } from "@/types/models"

/** 超限后的处置方式：首页徽章写全语义，避免“无操作”脱离上下文被误读 */
const ACTION_LABEL: Record<Rule["on_exceed"], string> = {
  notify_only: "超限无操作",
  graceful_close: "超限关闭前台",
  force_kill: "超限强制结束",
}

type CardBadge = {
  label: string
  variant: "secondary" | "destructive" | "outline"
  class: string
}

const monitor = useMonitorStore()
const rulesStore = useRulesStore()
const router = useRouter()

type RuleStatusFilter = "all" | "enabled" | "paused" | "closed"
type UsageFilter = "all" | "exceeded" | "normal"
type KindFilter = "all" | "focus" | "slack"
const ruleStatusFilter = ref<RuleStatusFilter>("all")
const usageFilter = ref<UsageFilter>("all")
const kindFilter = ref<KindFilter>("all")
const filterOpen = ref(false)

const toggleGroupClass = "inline-flex h-7 items-center gap-0.5"
const toggleItemClass =
  "h-7 cursor-pointer rounded px-2 text-[0.6875rem] whitespace-nowrap text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground data-[state=on]:bg-primary/10 data-[state=on]:text-primary"
/** 图标与应用名：与规则页、进程选择器共用同一份缓存 */
const { ensure: ensureMeta, iconOf, nameOf } = useAppMeta()

onMounted(async () => {
  if (rulesStore.rules.length === 0) await rulesStore.load()
})

const current = computed(() => monitor.current)
/** 当前前台的元信息 key：有完整路径就用路径，否则退回进程键 */
const currentKey = computed(() => current.value?.path ?? current.value?.exe_key ?? "")

watch(
  () => [currentKey.value, ...rulesStore.rules.map((r) => r.pattern)],
  (keys) => ensureMeta(keys),
  { immediate: true }
)

/** 规则 id → 顺序：概览顺序跟随拖拽结果，避免每秒快照把卡片弹回原顺序 */
const ruleOrder = computed(() => {
  const order = new Map<string, number>()
  rulesStore.rules.forEach((r, i) => order.set(r.id, i))
  return order
})

/** 概览卡片拖拽：与规则页共用同一份顺序 */
const sort = useCardSort({
  ids: () => rulesStore.rules.map((r) => r.id),
  commit: (ids) => rulesStore.reorder(ids),
})

/** id → 规则：过滤与分桶都要按 id 反查，先建 Map 避免逐个 find */
const ruleMap = computed(() => new Map(rulesStore.rules.map((r) => [r.id, r])))

/**
 * 卡片展示全部规则（含暂停与关闭），只有汇总数字排除暂停/关闭规则。
 * 规则列表还没加载时按启用处理，避免首帧没有数据。
 */
const allUsages = computed(() => monitor.ruleUsages)
const activeUsages = computed(() =>
  allUsages.value.filter((u) => {
    const rule = ruleMap.value.get(u.rule_id)
    return (rule?.enabled ?? true) && !(rule?.paused ?? false)
  })
)

/**
 *
 * @param rule
 */
function ruleStatusOf(rule: Rule | undefined): "enabled" | "paused" | "closed" {
  if (!rule || !rule.enabled) return "closed"
  return rule.paused ? "paused" : "enabled"
}

/**
 * 一条规则的时间线：
 * - 宽限中 → 「宽限倒计时」，结束后自动消失；
 * - 专注 + 配了休息间隔 → 循环倒计时「距下轮休息」；
 * - 开启超限提醒 → 「距超限」。
 * 都没有时返回空数组，卡片不占位。
 * @param usage
 * @param rule
 */
function timelinesOf(usage: RuleUsage, rule: Rule | undefined): RuleTimelineItem[] {
  if (!rule) return []
  const out: RuleTimelineItem[] = []

  const now = Date.now()
  // 无操作规则不做处置，但仍把已超出额度的时间显示出来
  if (rule.on_exceed === "notify_only" && usage.limit_ms > 0 && usage.used_ms >= usage.limit_ms) {
    out.push({
      id: "over",
      label: "超限时间",
      remainingMs: usage.used_ms - usage.limit_ms,
      totalMs: usage.limit_ms,
      percent: 100,
      tone: "danger",
    })
  }
  // 「无操作 + 关闭超限提醒」的规则不弹窗、不处置，宽限状态只是内部节流，不展示给用户
  const showsGrace = rule.on_exceed !== "notify_only" && rule.exceed_remind !== false
  if (showsGrace && usage.grace_deadline_ts > now) {
    const graceLeft = usage.grace_deadline_ts - now
    const graceTotal = Math.max(graceLeft, usage.grace_ms)
    out.push({
      id: "grace",
      label: "宽限倒计时",
      remainingMs: graceLeft,
      totalMs: graceTotal,
      percent: (graceLeft / graceTotal) * 100,
      tone: "danger",
    })
  }

  if (rule.kind === "focus" && rule.rest_remind_secs > 0) {
    const intervalMs = rule.rest_remind_secs * 1000
    // 累计时长每满一个间隔就提醒一次：取模得到当前这一轮的进度
    const restLeft = intervalMs - (usage.used_ms % intervalMs)
    out.push({
      id: "rest",
      label: "距下轮休息",
      remainingMs: restLeft,
      totalMs: intervalMs,
      percent: (restLeft / intervalMs) * 100,
      tone: "focus",
    })
  }

  if (rule.exceed_remind !== false && usage.limit_ms > 0) {
    const limitLeft = Math.max(0, usage.limit_ms - usage.used_ms)
    out.push({
      id: "limit",
      label: "距今日超限",
      remainingMs: limitLeft,
      totalMs: usage.limit_ms,
      percent: (usage.used_ms / usage.limit_ms) * 100,
      tone: usageTone(usage.used_ms, usage.limit_ms),
    })
  }

  return out
}

const rows = computed(() => {
  const order = ruleOrder.value
  const byId = ruleMap.value
  const weight = (ruleId: string) => order.get(ruleId) ?? Number.MAX_SAFE_INTEGER
  return allUsages.value
    .slice()
    .sort((a, b) => weight(a.rule_id) - weight(b.rule_id))
    .map((usage) => {
      const rule = byId.get(usage.rule_id)
      const ruleStatus = ruleStatusOf(rule)
      const isNotifyOnly = rule?.on_exceed === "notify_only"
      // 颜色只跟百分比走（阈值见 utils/usage），徽章文案按用户可见语义决定
      const tone = usageTone(usage.used_ms, usage.limit_ms)
      const overLimit = usage.limit_ms > 0 && usage.used_ms >= usage.limit_ms
      const badge: CardBadge | null =
        ruleStatus === "paused"
          ? {
              label: "暂停",
              variant: "outline",
              class: "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400",
            }
          : usage.state === "normal" && !overLimit
            ? null
            : isNotifyOnly
              ? overLimit
                ? {
                    label: "已超限",
                    variant: "secondary",
                    class: "border-transparent bg-muted text-muted-foreground",
                  }
                : null
              : usage.state === "warning"
                ? { label: "已超限", variant: "destructive", class: "" }
                : usage.state === "cleanup"
                  ? {
                      label: "等待后台清理",
                      variant: "outline",
                      class: "border-amber-500/40 text-amber-600 dark:text-amber-400",
                    }
                  : { label: "已处置", variant: "destructive", class: "" }
      // 无操作规则没有真的处置，卡片状态也归一为正常，避免红框与徽章冲突
      const displayUsage = isNotifyOnly ? { ...usage, state: "normal" as const } : usage
      const hasFullscreen = Boolean(
        (rule?.exceed_remind !== false && rule?.exceed?.fullscreen) || (rule?.kind === "focus" && rule.rest_remind_secs > 0 && rule.rest?.fullscreen)
      )
      return {
        usage: displayUsage,
        rule,
        ruleStatus,
        hasFullscreen,
        tone,
        badge,
        kind: rule?.kind ?? ("slack" as const),
        actionLabel: rule ? ACTION_LABEL[rule.on_exceed] : "未配置",
        timelines: timelinesOf(usage, rule),
      }
    })
})

/** 三组筛选只影响卡片列表；统计汇总保持全量口径，切换筛选不跳数字 */
const filteredRows = computed(() =>
  rows.value.filter((row) => {
    if (ruleStatusFilter.value !== "all" && row.ruleStatus !== ruleStatusFilter.value) {
      return false
    }
    if (kindFilter.value !== "all" && row.kind !== kindFilter.value) return false
    if (usageFilter.value === "all") return true
    const exceeded = ["warning", "enforced", "cleanup"].includes(row.usage.state)
    return usageFilter.value === "exceeded" ? exceeded : !exceeded
  })
)

/**
 *
 * @param rowId
 * @param paused
 */
async function togglePaused(rowId: string, paused: boolean): Promise<void> {
  await rulesStore.setPaused(rowId, paused)
}

/**
 *
 * @param rowId
 */
function jumpToRule(rowId: string): void {
  void router.push({ name: "rules", query: { focus: rowId } })
}

/**
 * 今日累计的三类互斥时长来自后端分类时间线，不再按规则用量求和；
 * 单条规则的额度仍由规则用量卡片独立展示。
 */
const categoryRows = computed(() => {
  const focus = monitor.focusTotalMs
  const slack = monitor.slackTotalMs
  const focusSlack = monitor.focusSlackTotalMs
  const total = focus + slack + focusSlack
  const percent = (value: number) => (total > 0 ? Math.round((value / total) * 100) : 0)
  return [
    {
      label: "专注",
      used: focus,
      percent: percent(focus),
      dot: "bg-emerald-500",
      text: "text-emerald-600 dark:text-emerald-400",
    },
    {
      label: "摸鱼",
      used: slack,
      percent: percent(slack),
      dot: "bg-amber-500",
      text: "text-amber-600 dark:text-amber-400",
    },
    {
      label: "专注摸鱼",
      used: focusSlack,
      percent: percent(focusSlack),
      dot: "bg-violet-500",
      text: "text-violet-600 dark:text-violet-400",
    },
  ]
})

const integrityLabel = computed(() => {
  const i = current.value?.integrity
  if (i === "denied") return "无法识别（受保护进程）"
  if (i === "unknown") return "仅识别到进程名"
  return "已识别"
})
</script>

<template>
  <div class="flex flex-col gap-4">
    <!-- 今日总览 -->
    <!-- minmax(0,1fr)：默认的 1fr 带 auto 最小尺寸，长路径会把整行撑出视口 -->
    <!-- 左列按内容自适应但至少 360px：环 148 + 间距 + 不换行的文案（约 143px）+ 卡片内边距 -->
    <div class="grid gap-4 lg:grid-cols-[minmax(360px,max-content)_minmax(0,1fr)]">
      <Card class="min-w-0">
        <CardHeader>
          <CardTitle class="text-sm">今日累计</CardTitle>
          <CardDescription class="text-xs"> 按时间轴去重，同一时刻只计一次；三类相加等于累计 </CardDescription>
        </CardHeader>
        <CardContent class="flex items-center gap-4">
          <DualUsageRing
            :focus-ms="monitor.focusTotalMs"
            :slack-ms="monitor.slackTotalMs"
            :focus-slack-ms="monitor.focusSlackTotalMs"
            :size="152"
            :value="formatDuration(monitor.todayTotalMs)"
            caption="今日累计 · 已去重"
          />
          <!-- whitespace-nowrap：左侧固定宽度里宁愿挤也别折行 -->
          <div class="w-47 min-w-0 space-y-2 whitespace-nowrap">
            <div v-for="item in categoryRows" :key="item.label" class="space-y-0.5">
              <div class="flex items-center gap-1.5 text-[0.8125rem]">
                <span class="size-2 shrink-0 rounded-full" :class="item.dot" />
                <span class="text-muted-foreground">{{ item.label }}</span>
                <span class="ml-auto font-mono text-xs font-medium tabular-nums" :class="item.text">
                  {{ formatDuration(item.used) }}
                </span>
              </div>
              <div class="text-muted-foreground pl-3.5 text-[0.6875rem]">占比 {{ item.percent }}%</div>
            </div>

            <Separator />

            <div class="flex flex-wrap items-center gap-1.5">
              <span class="text-muted-foreground text-[0.8125rem]"> 共 {{ activeUsages.length }} 条规则正在计时 </span>
              <Badge v-if="monitor.paused" variant="outline" class="border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400">
                计时已暂停
              </Badge>
              <Badge
                v-else-if="monitor.resting"
                variant="outline"
                class="border-emerald-500/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
              >
                休息中 · 计时冻结
              </Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card class="min-w-0 overflow-hidden">
        <CardHeader>
          <CardTitle class="text-sm">当前前台</CardTitle>
          <CardDescription class="text-xs">
            {{ current ? integrityLabel : "没有检测到前台窗口" }}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div v-if="current" class="flex flex-col gap-2">
            <div class="flex items-center gap-2">
              <img v-if="iconOf(currentKey)" :src="iconOf(currentKey)" alt="" class="size-6 shrink-0 rounded-sm" />
              <AppWindow v-else class="text-muted-foreground/60 size-6 shrink-0" />
              <span class="truncate text-sm font-medium">
                {{ nameOf(currentKey, current.exe_name) }}
              </span>
              <span v-if="nameOf(currentKey) && nameOf(currentKey) !== current.exe_name" class="text-muted-foreground shrink-0 font-mono text-xs">
                {{ current.exe_name }}
              </span>
              <Badge v-if="current.tracked" class="text-xs">已纳入专注</Badge>
              <Badge v-else variant="outline" class="text-xs">未纳入专注</Badge>
              <Badge v-if="current.integrity !== 'ok'" variant="destructive" class="text-xs">
                {{ current.integrity }}
              </Badge>
            </div>
            <div class="text-muted-foreground truncate text-xs" :title="current.title">
              {{ current.title || "（无标题）" }}
            </div>
            <div class="text-muted-foreground flex min-w-0 items-center gap-1 font-mono text-xs">
              <span class="shrink-0">PID {{ current.pid }} ·</span>
              <MiddleEllipsis :text="current.path ?? '路径不可读'" class="flex-1" />
            </div>
          </div>
          <div v-else class="text-muted-foreground text-xs">切到目标程序即可看到归属结果</div>
        </CardContent>
      </Card>
    </div>

    <!-- 规则用量 -->
    <div>
      <div class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-2.5">
        <h2 class="text-lg font-medium">规则用量</h2>
        <Button
          size="icon"
          variant="ghost"
          class="text-muted-foreground hover:text-foreground size-8 cursor-pointer"
          title="筛选规则"
          @click="filterOpen = !filterOpen"
        >
          <Filter class="size-4" />
        </Button>

        <template v-if="filterOpen">
          <div class="flex min-w-0 flex-wrap items-center gap-1.5">
            <div class="bg-muted/35 flex h-8 items-center gap-2 rounded-md border px-2">
              <span class="text-muted-foreground w-14 shrink-0 text-xs">规则状态</span>
              <ToggleGroupRoot
                type="single"
                :model-value="ruleStatusFilter"
                :class="toggleGroupClass"
                aria-label="按规则状态筛选"
                @update:model-value="(v) => (ruleStatusFilter = String(v) as RuleStatusFilter)"
              >
                <ToggleGroupItem :class="toggleItemClass" value="all">全部</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="enabled">启用</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="paused">暂停</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="closed">关闭</ToggleGroupItem>
              </ToggleGroupRoot>
            </div>
            <div class="bg-muted/35 flex h-8 items-center gap-2 rounded-md border px-2">
              <span class="text-muted-foreground w-14 shrink-0 text-xs">当前状态</span>
              <ToggleGroupRoot
                type="single"
                :model-value="usageFilter"
                :class="toggleGroupClass"
                aria-label="按当前状态筛选"
                @update:model-value="(v) => (usageFilter = String(v) as UsageFilter)"
              >
                <ToggleGroupItem :class="toggleItemClass" value="all">全部</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="exceeded">超限</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="normal">正常</ToggleGroupItem>
              </ToggleGroupRoot>
            </div>
            <div class="bg-muted/35 flex h-8 items-center gap-2 rounded-md border px-2">
              <span class="text-muted-foreground w-14 shrink-0 text-xs">类型</span>
              <ToggleGroupRoot
                type="single"
                :model-value="kindFilter"
                :class="toggleGroupClass"
                aria-label="按类型筛选"
                @update:model-value="(v) => (kindFilter = String(v) as KindFilter)"
              >
                <ToggleGroupItem :class="toggleItemClass" value="all">全部</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="focus">专注</ToggleGroupItem>
                <ToggleGroupItem :class="toggleItemClass" value="slack">摸鱼</ToggleGroupItem>
              </ToggleGroupRoot>
            </div>
          </div>
        </template>

        <Button as-child variant="ghost" size="sm" class="ml-auto h-8! px-2.5! text-sm!">
          <RouterLink :to="{ name: 'rules' }">管理规则</RouterLink>
        </Button>
      </div>

      <div v-if="filteredRows.length === 0" class="rounded-lg border border-dashed p-8 text-center">
        <ListChecks class="text-muted-foreground mx-auto mb-2 size-6" />
        <p class="text-muted-foreground text-xs">
          {{ rows.length === 0 ? "还没有规则。先加一条，莳间才知道该管谁。" : "当前筛选条件下没有规则，换一组条件试试。" }}
        </p>
        <Button as-child size="sm" class="mt-3">
          <RouterLink :to="{ name: 'rules' }">
            {{ rows.length === 0 ? "新建规则" : "管理规则" }}
          </RouterLink>
        </Button>
      </div>

      <div v-else class="grid gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
        <RuleUsageItem
          v-for="row in filteredRows"
          :key="row.usage.rule_id"
          :usage="row.usage"
          :rule="row.rule"
          :rule-status="row.ruleStatus"
          :has-fullscreen="row.hasFullscreen"
          :kind="row.kind"
          :tone="row.tone"
          :badge="row.badge"
          :action-label="row.actionLabel"
          :timelines="row.timelines"
          :card-attrs="sort.cardProps(row.usage.rule_id)"
          :handle-attrs="sort.handleProps(row.usage.rule_id)"
          :over="sort.isOver(row.usage.rule_id)"
          :dragging="sort.isDragging(row.usage.rule_id)"
          @toggle-paused="(paused: boolean) => togglePaused(row.usage.rule_id, paused)"
          @jump="jumpToRule(row.usage.rule_id)"
        />
      </div>
    </div>
  </div>
</template>
