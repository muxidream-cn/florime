import { computed, reactive, ref, type ComputedRef, type Ref } from "vue"

import { listRunningProcesses } from "@/api/commands"
import { useAppMeta } from "@/composables/useAppMeta"
import { blankRuleForm, looksLikePath, ruleToForm, validateRuleForm, type RuleForm } from "@/lib/ruleForm"

import type { MatchKind, ProcRow, Rule } from "@/types/models"

export interface RuleDraft {
  /** 表单本体：页面直接绑定，改哪项都只改这一份 */
  form: RuleForm
  /** 切换匹配方式时的行内提示（补全成功 / 找不到 / 有歧义） */
  kindHint: Ref<string>
  /** 正在按运行进程反查路径：期间不允许提交 */
  resolving: Ref<boolean>
  /** 与最后一次 markClean 相比是否有改动 */
  dirty: ComputedRef<boolean>
  /** 前端即时校验：空串表示可以保存 */
  errorText: ComputedRef<string>
  /** 回填：传规则走编辑，不传走新建；同时把当前状态记为干净 */
  fill: (rule?: Rule | null) => void
  /** 保存成功后调用，把当前状态记为干净 */
  markClean: () => void
  onMatchKindChange: (next: MatchKind) => Promise<void>
  onPicked: (row: ProcRow) => void
}

/**
 * 规则表单的草稿域：状态、回填、字段联动与脏标记。
 *
 * 只负责「表单里发生了什么」，不碰 store 写入和路由跳转——
 * 保存、离开确认留在页面里，这样以后导入规则想复用同一份表单也不会被导航逻辑绑住。
 * 脏判断用序列化快照比对：改回原值就不算脏，比「动过就算脏」更贴近用户预期。
 */
export function useRuleDraft(): RuleDraft {
  const { ensure: ensureMeta, nameOf } = useAppMeta()

  const form = reactive<RuleForm>(blankRuleForm())
  const kindHint = ref("")
  const resolving = ref(false)

  const snapshot = (): string => JSON.stringify(form)
  /** 干净态快照；初值取空白表单，保证没调用 fill 时也不会误判为脏 */
  const baseline = ref(snapshot())

  /**
   *
   */
  function markClean(): void {
    baseline.value = snapshot()
  }

  /**
   *
   * @param rule
   */
  function fill(rule?: Rule | null): void {
    kindHint.value = ""
    Object.assign(form, rule ? ruleToForm(rule) : blankRuleForm())
    markClean()
  }

  const errorText = computed(() => validateRuleForm(form))
  const dirty = computed(() => baseline.value !== snapshot())

  /**
   * 切换匹配方式时把「目标」补成对应形态：
   * 只填了 exe 名却切成完整路径，规则会永远匹配不上，所以这里按当前运行进程反查路径。
   * @param next
   */
  async function onMatchKindChange(next: MatchKind): Promise<void> {
    if (next === form.match_kind) return
    form.match_kind = next
    kindHint.value = ""

    const pattern = form.pattern.trim()
    if (!pattern) return

    if (next === "exe_name") {
      if (looksLikePath(pattern)) {
        form.pattern = pattern.split(/[\\/]/).pop() ?? pattern
        kindHint.value = "已只保留文件名"
      }
      return
    }

    if (looksLikePath(pattern)) return

    resolving.value = true
    try {
      const rows = await listRunningProcesses({ excludeProtected: false, ignoreSystem: false })
      const hits = rows.filter((r) => r.path !== null && r.exe_name.toLowerCase() === pattern.toLowerCase())
      if (hits.length === 1) {
        form.pattern = hits[0].path as string
        kindHint.value = "已按当前运行进程补全路径"
      } else if (hits.length > 1) {
        form.pattern = hits[0].path as string
        kindHint.value = `有 ${hits.length} 个同名进程，已填入其中一个，建议用「选择」确认`
      } else {
        kindHint.value = "没找到同名运行进程：请用「选择」挑一个，或改回「仅进程名」"
      }
    } catch (e) {
      console.error("补全路径失败", e)
      kindHint.value = "读取进程列表失败，请用「选择」挑一个"
    } finally {
      resolving.value = false
    }
  }

  /**
   * 选择器回来的进程：有完整路径就按路径匹配，否则降级为进程名
   * @param row
   */
  function onPicked(row: ProcRow): void {
    kindHint.value = ""
    if (row.path) {
      form.pattern = row.path
      form.match_kind = "full_path"
    } else {
      form.pattern = row.exe_name
      form.match_kind = "exe_name"
    }
    if (!form.display_name.trim()) {
      const key = row.path ?? row.exe_name
      ensureMeta([key, row.exe_name])
      // 元信息可能还在路上：先用手上的缓存，取不到再回退 exe 名
      form.display_name = nameOf(key) || nameOf(row.exe_name) || row.exe_name.replace(/\.exe$/i, "")
    }
  }

  return {
    form,
    kindHint,
    resolving,
    dirty,
    errorText,
    fill,
    markClean,
    onMatchKindChange,
    onPicked,
  }
}
