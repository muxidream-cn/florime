import { defineStore } from "pinia"
import { computed, ref } from "vue"

import * as api from "@/api/commands"

import type { Rule, RuleInput } from "@/types/models"

export const useRulesStore = defineStore("rules", () => {
  const rules = ref<Rule[]>([])
  const loading = ref(false)
  const error = ref("")

  const enabledCount = computed(() => rules.value.filter((r) => r.enabled).length)

  /**
   *
   */
  async function load(): Promise<void> {
    loading.value = true
    try {
      rules.value = await api.getRules()
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  /**
   *
   * @param input
   */
  async function save(input: RuleInput): Promise<boolean> {
    error.value = ""
    try {
      rules.value = await api.upsertRule(input)
      return true
    } catch (e) {
      error.value = errText(e)
      return false
    }
  }

  /**
   *
   * @param id
   */
  async function remove(id: string): Promise<void> {
    try {
      rules.value = await api.deleteRule(id)
    } catch (e) {
      error.value = errText(e)
    }
  }

  /**
   * 拖拽排序：先本地乐观重排让两处视图立刻一致，再落盘；失败回滚并与服务端对齐
   * @param ids
   */
  async function reorder(ids: string[]): Promise<void> {
    error.value = ""
    const prev = rules.value
    const byId = new Map(prev.map((r) => [r.id, r]))
    const next: Rule[] = []
    for (const id of ids) {
      const hit = byId.get(id)
      if (hit) next.push(hit)
    }
    // 没传到的规则（理论上不会发生）补在末尾，绝不因为排序丢规则
    for (const r of prev) {
      if (!next.includes(r)) next.push(r)
    }
    rules.value = next

    try {
      rules.value = await api.reorderRules(ids)
    } catch (e) {
      rules.value = prev
      error.value = errText(e)
      // 本地已经不是服务端的顺序了，重新拉一次兜底
      void load()
    }
  }

  /**
   *
   * @param id
   * @param enabled
   */
  async function toggle(id: string, enabled: boolean): Promise<void> {
    // 乐观更新，失败回滚
    const target = rules.value.find((r) => r.id === id)
    if (!target) return
    const prev = target.enabled
    target.enabled = enabled
    try {
      rules.value = await api.setRuleEnabled(id, enabled)
    } catch (e) {
      target.enabled = prev
      error.value = errText(e)
    }
  }

  /**
   * 暂停/恢复单条规则：乐观更新，失败回滚到服务端结果
   * @param id
   * @param paused
   */
  async function setPaused(id: string, paused: boolean): Promise<void> {
    const target = rules.value.find((r) => r.id === id)
    if (!target) return
    const prev = target.paused
    target.paused = paused
    try {
      rules.value = await api.setRulePaused(id, paused)
    } catch (e) {
      target.paused = prev
      error.value = errText(e)
      await load()
    }
  }

  /**
   *
   * @param id
   */
  function byId(id: string): Rule | undefined {
    return rules.value.find((r) => r.id === id)
  }

  return {
    rules,
    loading,
    error,
    enabledCount,
    load,
    save,
    remove,
    reorder,
    toggle,
    setPaused,
    byId,
  }
})

/**
 *
 * @param e
 */
function errText(e: unknown): string {
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message)
  }
  return String(e)
}
