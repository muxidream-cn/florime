<script setup lang="ts">
import { ArrowLeft, ListChecks } from "@lucide/vue"
import { computed, ref, watch } from "vue"
import { onBeforeRouteLeave, useRoute, useRouter } from "vue-router"

import RuleKindTabs from "@/components/RuleKindTabs.vue"
import RuleReminderSection from "@/components/RuleReminderSection.vue"
import TargetPicker from "@/components/TargetPicker.vue"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { useRuleDraft } from "@/composables/useRuleDraft"
import { formToInput } from "@/lib/ruleForm"
import { useRulesStore } from "@/stores/rules"

import type { MatchKind } from "@/types/models"

/**
 * 规则编辑页（/rules/new 与 /rules/:id 共用）。
 *
 * 表单域与字段联动在 `useRuleDraft` 里，这里只管三件事：按路由装载规则、
 * 保存后回列表、离开前拦住未保存的改动。
 * 之所以不再用弹窗：右列提醒配置有近二十个控件，弹窗的 90vh + 列内滚动太挤。
 */
const route = useRoute()
const router = useRouter()
const rulesStore = useRulesStore()

const { form, kindHint, resolving, dirty, errorText, fill, markClean, onMatchKindChange, onPicked } = useRuleDraft()

const isEdit = computed(() => route.name === "rule-edit")
const loading = ref(true)
const notFound = ref(false)
const submitting = ref(false)
/** 保存成功或用户确认放弃后放行，避免自己的守卫把跳转拦回来 */
const leaving = ref(false)
const discardOpen = ref(false)
/** 被守卫拦下的目标路径：确认放弃后按它继续跳 */
const pendingPath = ref("")

/** 快速连点不同规则时，用令牌丢弃过期的装载结果 */
let loadToken = 0

/**
 *
 */
async function init(): Promise<void> {
  const token = ++loadToken
  loading.value = true
  notFound.value = false

  const id = isEdit.value ? String(route.params.id ?? "") : ""
  // 深链直接进编辑页时 store 可能还是空的：补一次，之后与列表页共用同一份数据
  if (id && rulesStore.rules.length === 0) await rulesStore.load()
  if (token !== loadToken) return

  const rule = id ? rulesStore.byId(id) : undefined
  notFound.value = !!id && !rule
  fill(rule ?? null)
  loading.value = false
}

watch(
  () => route.params.id,
  () => void init(),
  { immediate: true }
)

/**
 *
 */
async function submit(): Promise<void> {
  if (errorText.value || resolving.value || submitting.value) return
  submitting.value = true
  const ok = await rulesStore.save(formToInput(form))
  submitting.value = false
  if (!ok) return
  // save 已用返回的列表刷新 store，无需再 load
  markClean()
  leaving.value = true
  await router.push({ name: "rules" })
}

/**
 *
 */
function goBack(): void {
  void router.push({ name: "rules" })
}

onBeforeRouteLeave((to) => {
  if (leaving.value || !dirty.value) return true
  pendingPath.value = to.fullPath
  discardOpen.value = true
  return false
})

/**
 * 只关确认框，不清 pendingPath：AlertDialogAction 先触发关闭、后触发点击
 * @param open
 */
function onDiscardOpenChange(open: boolean): void {
  if (open) return
  discardOpen.value = false
}

/**
 *
 */
function stayOnPage(): void {
  discardOpen.value = false
  pendingPath.value = ""
}

/**
 *
 */
async function discardAndLeave(): Promise<void> {
  const target = pendingPath.value || "/rules"
  pendingPath.value = ""
  discardOpen.value = false
  leaving.value = true
  await router.push(target)
}
</script>

<template>
  <!-- fullBleed 路由：main 不给内边距，水平留白与顶部间距由页面自己负责 -->
  <div class="flex h-full flex-col gap-4 px-5 pt-5">
    <!-- 页头：返回 + 标题 -->
    <div class="flex shrink-0 items-start gap-2">
      <Button variant="ghost" size="icon-sm" class="mt-0.5 shrink-0 cursor-pointer" aria-label="返回规则列表" @click="goBack">
        <ArrowLeft />
      </Button>
      <div class="min-w-0">
        <h1 class="text-base font-semibold">{{ isEdit ? "编辑规则" : "新建规则" }}</h1>
        <p class="text-muted-foreground text-xs">
          {{ isEdit ? "改完保存才会生效。" : "填完保存后立即参与计时与提醒。" }}
        </p>
      </div>
    </div>

    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton class="h-16 w-full" />
      <Skeleton class="h-64 w-full" />
      <Skeleton class="h-32 w-full" />
    </div>

    <div v-else-if="notFound" class="rounded-lg border border-dashed p-10 text-center">
      <ListChecks class="text-muted-foreground mx-auto mb-2 size-6" />
      <p class="text-muted-foreground text-xs">规则不存在或已被删除。</p>
      <Button size="sm" class="mt-3" @click="goBack">返回规则列表</Button>
    </div>

    <template v-else>
      <RuleKindTabs v-model="form.kind" class="shrink-0" />

      <div class="grid min-h-0 flex-1 items-start gap-4 overflow-y-auto lg:flex lg:items-stretch lg:overflow-hidden">
        <!-- 左：基础信息 -->
        <Card class="lg:min-h-0 lg:flex-1">
          <CardHeader>
            <CardTitle class="text-sm">基础信息</CardTitle>
            <CardDescription class="text-xs">决定管谁、怎么计时。</CardDescription>
          </CardHeader>
          <CardContent class="flex flex-col gap-3">
            <div class="flex flex-col gap-1.5">
              <Label class="text-xs">名称</Label>
              <Input v-model="form.display_name" placeholder="例如：某游戏" class="h-8 text-xs" />
            </div>

            <div class="flex flex-col gap-1.5">
              <Label class="text-xs">目标</Label>
              <TargetPicker v-model:pattern="form.pattern" @picked="onPicked" />
              <p v-if="kindHint" class="text-muted-foreground text-xs">{{ kindHint }}</p>
            </div>

            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-1.5">
                <Label class="text-xs">匹配方式</Label>
                <Select :model-value="form.match_kind" @update:model-value="(v) => onMatchKindChange(v as MatchKind)">
                  <SelectTrigger class="h-8 w-full text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="full_path">完整路径</SelectItem>
                    <SelectItem value="exe_name">仅进程名</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div class="flex flex-col gap-1.5">
                <Label class="text-xs">计时方式</Label>
                <Select v-model="form.mode">
                  <SelectTrigger class="h-8 w-full text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="foreground">仅前台计时</SelectItem>
                    <SelectItem value="running">存在即计时</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div class="flex items-center justify-between rounded-md border px-3 py-2">
              <div class="text-xs">启用这条规则</div>
              <Switch v-model="form.enabled" />
            </div>

            <p class="text-muted-foreground text-[0.6875rem] leading-relaxed">每日上限与宽限期归在右侧「超限提醒」里：它们只决定超额后怎么处置。</p>
          </CardContent>
        </Card>

        <!-- 右：提醒与处置 -->
        <Card class="lg:min-h-0 lg:flex-1 lg:overflow-y-auto">
          <CardHeader>
            <CardTitle class="text-sm">提醒与处置</CardTitle>
            <CardDescription class="text-xs">额度用满后先提醒、再按处置方式收尾。</CardDescription>
          </CardHeader>
          <CardContent>
            <RuleReminderSection
              v-model:kind="form.kind"
              v-model:limit-minutes="form.limit_minutes"
              v-model:grace-secs="form.grace_secs"
              v-model:rest-minutes="form.rest_minutes"
              v-model:rest-auto-continue="form.rest_auto_continue"
              v-model:rest="form.rest"
              v-model:exceed="form.exceed"
              v-model:exceed-on="form.exceed_on"
              v-model:action="form.on_exceed"
              v-model:allow-background="form.allow_background"
            />
          </CardContent>
        </Card>
      </div>

      <!-- 吸底操作栏：错误常驻左侧，按钮固定右侧 -->
      <div class="bg-background/95 z-10 -mx-5 flex shrink-0 items-center justify-between gap-3 border-t px-5 py-3 backdrop-blur">
        <p v-if="errorText" class="text-destructive min-w-0 truncate text-[0.8125rem]" :title="errorText">
          {{ errorText }}
        </p>
        <p v-else-if="rulesStore.error" class="text-destructive min-w-0 truncate text-[0.8125rem]" :title="rulesStore.error">
          {{ rulesStore.error }}
        </p>
        <p v-else class="text-muted-foreground min-w-0 truncate text-[0.6875rem]">保存后立即生效；未保存离开会先确认。</p>

        <div class="flex shrink-0 items-center gap-2">
          <Button variant="ghost" size="sm" @click="goBack">取消</Button>
          <Button size="sm" :disabled="!!errorText || submitting || resolving" @click="submit">
            {{ submitting ? "保存中…" : "保存" }}
          </Button>
        </div>
      </div>
    </template>

    <AlertDialog :open="discardOpen" @update:open="onDiscardOpenChange">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle class="text-sm">放弃未保存的修改？</AlertDialogTitle>
          <AlertDialogDescription class="text-xs"> 当前表单还没保存，离开后这些改动会丢失。 </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel size="sm" @click="stayOnPage">继续编辑</AlertDialogCancel>
          <AlertDialogAction size="sm" class="bg-destructive hover:bg-destructive/90 text-white" @click="discardAndLeave">
            放弃修改
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
