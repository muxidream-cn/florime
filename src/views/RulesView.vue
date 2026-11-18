<script setup lang="ts">
import { AppWindow, Plus, X } from "@lucide/vue"
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { useRoute, useRouter } from "vue-router"

import ProcessPicker from "@/components/ProcessPicker.vue"
import RuleCard from "@/components/RuleCard.vue"
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
import { Input } from "@/components/ui/input"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAppMeta } from "@/composables/useAppMeta"
import { useCardSort } from "@/composables/useCardSort"
import { useRulesStore } from "@/stores/rules"
import { useSettingsStore } from "@/stores/settings"

import type { ProcRow, Rule } from "@/types/models"

const rulesStore = useRulesStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const route = useRoute()

/** 首页跳转过来的规则：短暂高亮，不打开编辑页 */
const highlightId = ref("")
let highlightTimer: number | undefined

const pendingDelete = ref<Rule | null>(null)
/**
 * 真正要删的 id 必须单独存一份。
 *
 * AlertDialogAction 本质是 DialogClose：点击时先同步触发 `update:open(false)`，
 * 那一刻 `pendingDelete` 已经被清空，再读它就永远是 null —— 这就是之前「点了删除没反应」的原因。
 */
const pendingDeleteId = ref("")
const deleting = ref(false)
const whiteInput = ref("")
const blackInput = ref("")

/** 图标与应用名：与进程选择器共用同一份缓存 */
const { ensure: ensureMeta, iconOf, nameOf } = useAppMeta()
const pickerOpen = ref(false)
const pickingList = ref<"whitelist" | "blacklist">("whitelist")

/**
 *
 */
async function focusRuleFromQuery(): Promise<void> {
  const id = typeof route.query.focus === "string" ? route.query.focus : ""
  if (!id) return
  await nextTick()
  const el = document.getElementById(`rule-${id}`)
  if (!el) return
  el.scrollIntoView({ behavior: "smooth", block: "center" })
  highlightId.value = id
  if (highlightTimer !== undefined) window.clearTimeout(highlightTimer)
  highlightTimer = window.setTimeout(() => {
    highlightId.value = ""
  }, 2000)
}

onMounted(async () => {
  await Promise.all([rulesStore.load(), settingsStore.load()])
  await focusRuleFromQuery()
})

watch(
  () => route.query.focus,
  () => void focusRuleFromQuery()
)

onBeforeUnmount(() => {
  if (highlightTimer !== undefined) window.clearTimeout(highlightTimer)
})

/** 规则目标与名单项变了才补元信息，避免每次渲染都发请求 */
const metaKeys = computed(() => [...rulesStore.rules.map((r) => r.pattern), ...settingsStore.settings.whitelist, ...settingsStore.settings.blacklist])
watch(metaKeys, (keys) => ensureMeta(keys), { immediate: true })

/** 规则卡拖拽排序：与概览页共用同一份顺序，落盘由 store 负责 */
const sort = useCardSort({
  ids: () => rulesStore.rules.map((r) => r.id),
  commit: (ids) => rulesStore.reorder(ids),
})

/**
 *
 * @param key
 */
function openPicker(key: "whitelist" | "blacklist"): void {
  pickingList.value = key
  pickerOpen.value = true
}

/**
 * 选择器回来的条目：优先完整路径，读不到路径才按进程名
 * @param row
 */
async function onListPicked(row: ProcRow): Promise<void> {
  const key = pickingList.value
  const raw = row.path ?? row.exe_name
  if (!raw) return
  const next = [...settingsStore.settings[key]]
  if (!next.some((v) => v.toLowerCase() === raw.toLowerCase())) next.push(raw)
  await settingsStore.patch({ [key]: next })
}

/**
 *
 * @param rule
 */
function askDelete(rule: Rule): void {
  pendingDelete.value = rule
  pendingDeleteId.value = rule.id
}

/**
 *
 * @param open
 */
function onDeleteOpenChange(open: boolean): void {
  if (open) return
  // 取消 / Esc / 点遮罩：只收弹窗，不动待删 id（真正删除由 confirmDelete 负责）
  pendingDelete.value = null
}

/**
 *
 */
async function confirmDelete(): Promise<void> {
  const id = pendingDeleteId.value
  if (!id || deleting.value) return
  deleting.value = true
  await rulesStore.remove(id)
  deleting.value = false
  pendingDeleteId.value = ""
  pendingDelete.value = null
}

const lists = computed(() => [
  { key: "whitelist" as const, label: "白名单", hint: "永不计时，优先级最高", value: settingsStore.settings.whitelist },
  { key: "blacklist" as const, label: "黑名单", hint: "完全不监督，也不参与处置", value: settingsStore.settings.blacklist },
])

/**
 *
 * @param key
 */
async function addToList(key: "whitelist" | "blacklist"): Promise<void> {
  const raw = (key === "whitelist" ? whiteInput.value : blackInput.value).trim()
  if (!raw) return
  const next = [...settingsStore.settings[key]]
  if (!next.some((v) => v.toLowerCase() === raw.toLowerCase())) next.push(raw)
  const ok = await settingsStore.patch({ [key]: next })
  if (ok) {
    if (key === "whitelist") whiteInput.value = ""
    else blackInput.value = ""
  }
}

/**
 *
 * @param key
 * @param value
 */
async function removeFromList(key: "whitelist" | "blacklist", value: string): Promise<void> {
  const next = settingsStore.settings[key].filter((v) => v !== value)
  await settingsStore.patch({ [key]: next })
}
</script>

<template>
  <div class="flex h-full flex-col gap-4">
    <div class="flex shrink-0 items-center justify-between">
      <div>
        <h1 class="text-base font-semibold">规则</h1>
        <p class="text-muted-foreground text-xs">规则决定监督谁、怎么计时、超限后怎么处理。</p>
      </div>
      <!-- 后续的「导入 / 导出」按钮就放在这一行，与新建规则并排 -->
      <div class="flex shrink-0 items-center gap-2">
        <Button as-child size="sm">
          <RouterLink :to="{ name: 'rule-new' }">
            <Plus data-icon="inline-start" />
            新建规则
          </RouterLink>
        </Button>
      </div>
    </div>

    <Tabs default-value="rules" class="min-h-0 flex-1">
      <TabsList class="shrink-0">
        <TabsTrigger value="rules">专注规则</TabsTrigger>
        <TabsTrigger value="whitelist">白名单</TabsTrigger>
        <TabsTrigger value="blacklist">黑名单</TabsTrigger>
      </TabsList>

      <TabsContent value="rules" class="mt-3 min-h-0 overflow-y-auto">
        <div v-if="rulesStore.rules.length === 0" class="rounded-lg border border-dashed p-10 text-center">
          <p class="text-muted-foreground text-xs">还没有规则。点右上角"新建规则"。</p>
        </div>

        <div v-else class="flex flex-col gap-2 px-2">
          <RuleCard
            v-for="rule in rulesStore.rules"
            :key="rule.id"
            :rule="rule"
            :highlight="highlightId === rule.id"
            :card-attrs="sort.cardProps(rule.id)"
            :handle-attrs="sort.handleProps(rule.id)"
            :over="sort.isOver(rule.id)"
            :dragging="sort.isDragging(rule.id)"
            @toggle="(v: boolean) => rulesStore.toggle(rule.id, v)"
            @edit="router.push({ name: 'rule-edit', params: { id: rule.id } })"
            @remove="askDelete(rule)"
          />
        </div>

        <p v-if="rulesStore.error" class="text-destructive mt-2 text-[0.8125rem]">
          {{ rulesStore.error }}
        </p>
      </TabsContent>

      <TabsContent v-for="list in lists" :key="list.key" :value="list.key" class="mt-3 min-h-0 overflow-y-auto">
        <div class="text-muted-foreground mb-2 text-xs">{{ list.hint }}</div>

        <div class="flex gap-2">
          <Input
            :model-value="list.key === 'whitelist' ? whiteInput : blackInput"
            :placeholder="list.key === 'whitelist' ? '例如：code.exe 或完整路径' : '例如：game.exe'"
            class="h-8 flex-1 font-mono text-[0.8125rem]"
            @update:model-value="
              (v: string | number) => {
                if (list.key === 'whitelist') whiteInput = String(v)
                else blackInput = String(v)
              }
            "
            @keydown.enter="addToList(list.key)"
          />
          <Button size="sm" variant="outline" @click="addToList(list.key)">添加</Button>
          <Button size="sm" variant="outline" @click="openPicker(list.key)">选择进程</Button>
        </div>

        <div v-if="list.value.length === 0" class="text-muted-foreground mt-3 rounded-md border border-dashed p-6 text-center text-[0.8125rem]">
          列表为空
        </div>

        <div v-else class="mt-3 flex flex-col gap-1">
          <div v-for="entry in list.value" :key="entry" class="flex items-center justify-between gap-3 rounded-md border px-3 py-1.5">
            <div class="flex min-w-0 items-center gap-2">
              <img v-if="iconOf(entry)" :src="iconOf(entry)" alt="" class="size-4 shrink-0 rounded-sm" />
              <AppWindow v-else class="text-muted-foreground/60 size-4 shrink-0" />
              <div class="min-w-0">
                <div class="truncate text-[0.8125rem]">{{ nameOf(entry, entry) }}</div>
                <div v-if="nameOf(entry) && nameOf(entry) !== entry" class="text-muted-foreground truncate font-mono text-xs">
                  {{ entry }}
                </div>
              </div>
            </div>
            <Button variant="ghost" size="icon-xs" @click="removeFromList(list.key, entry)">
              <X />
            </Button>
          </div>
        </div>
      </TabsContent>
    </Tabs>

    <ProcessPicker v-model:open="pickerOpen" @select="onListPicked" />

    <AlertDialog :open="!!pendingDelete" @update:open="onDeleteOpenChange">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle class="text-sm">删除规则？</AlertDialogTitle>
          <AlertDialogDescription class="text-xs"> 将删除「{{ pendingDelete?.display_name }}」。已经记录的时长不会被删掉。 </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel size="sm" @click="pendingDeleteId = ''">取消</AlertDialogCancel>
          <AlertDialogAction size="sm" :disabled="deleting" @click="confirmDelete">
            {{ deleting ? "删除中…" : "删除" }}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
