<script setup lang="ts">
import { AlarmClock, ShieldAlert } from "@lucide/vue"
import { computed } from "vue"

import NotifyConfigFields from "@/components/NotifyConfigFields.vue"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

import type { ExceedAction, NotifyConfig, RuleKind } from "@/types/models"

/**
 * 规则编辑弹窗的右列：休息提醒（专注模式）与超限提醒 / 处置（两种模式共用）。
 *
 * 拆出来是为了让主弹窗保持单文件体量可控，同时把「提醒」相关的联动
 * 收敛在一处：休息提醒的开关只由 `restMinutes > 0` 表达，不做双重状态。
 */
const kind = defineModel<RuleKind>("kind", { required: true })
/** 每日上限与宽限期：属于「超额后怎么处置」，所以跟着超限提醒放在右列 */
const limitMinutes = defineModel<number>("limitMinutes", { required: true })
const graceSecs = defineModel<number>("graceSecs", { required: true })
const restMinutes = defineModel<number>("restMinutes", { required: true })
/** 休息结束后是否自动继续计时；关闭时会在休息末尾询问用户 */
const restAutoContinue = defineModel<boolean>("restAutoContinue", { required: true })
const rest = defineModel<NotifyConfig>("rest", { required: true })
const exceed = defineModel<NotifyConfig>("exceed", { required: true })
const action = defineModel<ExceedAction>("action", { required: true })
/** 关闭前台后是否容忍后台残留；不允许时清理弹窗会提供强制关闭入口 */
const allowBackground = defineModel<boolean>("allowBackground", { required: true })
/** 超限时是否弹提醒窗；关闭只是不提醒，处置照常 */
const exceedOn = defineModel<boolean>("exceedOn", { required: true })

const isFocus = computed(() => kind.value === "focus")
const restOn = computed({
  get: () => restMinutes.value > 0,
  set: (on: boolean) => {
    restMinutes.value = on ? Math.max(15, restMinutes.value) : 0
  },
})

const accentText = computed(() => (isFocus.value ? "text-emerald-600" : "text-amber-600"))
const accentRing = computed(() => (isFocus.value ? "border-emerald-500/30" : "border-amber-500/30"))

const resetSwitch = () => {
  allowBackground.value = false
}

/**
 *
 * @param value
 */
function onRestMinutes(value: string | number): void {
  const n = Number(value)
  restMinutes.value = Number.isFinite(n) ? Math.max(0, Math.round(n)) : 0
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <!-- 休息提醒：专注模式专属 -->
    <section v-if="isFocus" class="bg-card/60 rounded-lg border p-3 transition-colors" :class="accentRing">
      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-2">
          <AlarmClock class="mt-0.5 size-4 shrink-0" :class="accentText" />
          <div class="min-w-0">
            <div class="text-xs font-medium">休息提醒</div>
            <div class="text-muted-foreground text-[0.6875rem]">
              {{ restOn ? `每累计 ${restMinutes} 分钟触发一次，提醒期间暂停计时` : "按当日累计使用时长定时提醒休息" }}
            </div>
          </div>
        </div>
        <Switch v-model="restOn" />
      </div>

      <div v-if="restOn" class="mt-3 flex flex-col gap-3 border-t pt-3">
        <div class="flex items-center justify-between gap-3">
          <Label class="text-xs">提醒间隔（分钟）</Label>
          <Input
            :model-value="restMinutes"
            type="number"
            min="5"
            max="480"
            class="h-8 w-20 text-center text-xs tabular-nums"
            @update:model-value="onRestMinutes"
          />
        </div>
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="text-xs font-medium">休息结束后自动继续专注</div>
            <div class="text-muted-foreground text-[0.6875rem]">
              {{ restAutoContinue ? "到点自动恢复计时" : "到点先询问是否继续努力" }}
            </div>
          </div>
          <Switch v-model="restAutoContinue" />
        </div>

        <!-- 休息提醒到点必收：持续时间始终有效，不跟「不可关闭」走 -->
        <NotifyConfigFields v-model="rest" auto-close-always />
      </div>
      <p v-else class="text-muted-foreground mt-2 text-[0.6875rem]">打开后每满 N 分钟弹出一次提醒，切走窗口不会重置。</p>
    </section>

    <!-- 超限提醒：两种模式都保留额度与处置 -->
    <section class="bg-card/60 rounded-lg border p-3" :class="accentRing">
      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-2">
          <ShieldAlert class="mt-0.5 size-4 shrink-0" :class="accentText" />
          <div class="min-w-0">
            <div class="text-xs font-medium">超限提醒</div>
            <div class="text-muted-foreground text-[0.6875rem]">
              {{ exceedOn ? "当日额度用满后的处置与提醒方式" : "额度用满后静默处置，不弹提醒窗" }}
            </div>
          </div>
        </div>
        <Switch v-model="exceedOn" />
      </div>

      <div class="mt-3 flex flex-col gap-3 border-t pt-3">
        <!-- 额度与宽限：先讲清「什么时候算超限」，再讲超限后怎么处置 -->
        <div class="flex flex-col gap-1.5">
          <div class="grid grid-cols-2 gap-3">
            <div class="flex flex-col gap-1.5">
              <Label class="text-xs">每日上限（分钟）</Label>
              <Input v-model.number="limitMinutes" type="number" min="1" class="h-8 text-xs" />
            </div>
            <div class="flex flex-col gap-1.5">
              <Label class="text-xs">宽限期（秒）</Label>
              <Input v-model.number="graceSecs" type="number" min="0" class="h-8 text-xs" />
            </div>
          </div>
          <p class="text-muted-foreground text-[0.6875rem] leading-relaxed">
            上限按当日累计计时；到点先给宽限期，宽限结束后才按下面的「超限后」处置。
          </p>
        </div>

        <div class="flex items-center justify-between gap-3">
          <Label class="text-xs">超限后</Label>
          <Select v-model="action">
            <SelectTrigger class="h-8 w-36 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="notify_only" @click="resetSwitch">无操作</SelectItem>
              <SelectItem value="graceful_close">关闭前台</SelectItem>
              <SelectItem value="force_kill" @click="resetSwitch">强制结束进程</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- 后台残留策略：允许则处置后静默收尾；不允许则保留清理提醒与强制关闭入口 -->
        <div
          class="flex items-center justify-between gap-3 rounded-md border border-dashed px-2.5 py-2 pl-4"
          :class="action === 'graceful_close' ? 'opacity-100' : 'opacity-50'"
        >
          <div class="min-w-0">
            <div class="text-xs font-medium">允许保留后台进程</div>
            <div class="text-muted-foreground text-[0.6875rem]">
              {{ allowBackground ? "关闭前台后若仍有后台进程，不再弹窗提醒" : "关闭前台后若仍有后台进程，继续提醒直到清理完成" }}
            </div>
          </div>
          <Switch v-model="allowBackground" :disabled="action !== 'graceful_close'" />
        </div>

        <!-- 提醒呈现配置只在开启时展开；关闭后仍保留额度、处置与后台策略 -->
        <NotifyConfigFields v-if="exceedOn" v-model="exceed" />
        <p v-else class="text-[0.6875rem] leading-relaxed text-amber-500">未开启提醒，执行时不会弹提醒窗、不发系统通知。</p>
      </div>
    </section>
  </div>
</template>
