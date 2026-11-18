import { ref } from "vue"

import { getAppMeta } from "@/api/commands"

import type { AppMetaMap } from "@/types/models"

/**
 * 程序图标与应用名的共享缓存。
 *
 * 状态放在模块级：进程选择器、规则页、概览拿的是同一批 key（exe 路径或 exe 名），
 * 共用一份缓存后才能做到「选择器里已经取过的，规则卡直接复用」。
 * 这里再按帧合并成一次批量请求，避免每个组件各发一遍、也避免每秒重取。
 * 后端本身还有进程内缓存，所以重复问同一个 key 几乎零成本。
 */
const meta = ref<AppMetaMap>({})
const loading = ref(false)
let pending = new Set<string>()
let timer: number | undefined

/**
 *
 */
async function flush(): Promise<void> {
  const keys = [...pending]
  pending = new Set()
  timer = undefined
  if (keys.length === 0) return
  loading.value = true
  try {
    const map = await getAppMeta(keys)
    meta.value = { ...meta.value, ...map }
  } catch (e) {
    console.error("读取程序图标/名称失败", e)
  } finally {
    loading.value = false
  }
}

/**
 * 登记需要元信息的 key；同一批次只发一次请求
 * @param keys
 */
function ensure(keys: (string | null | undefined)[]): void {
  for (const key of keys) {
    if (!key || key in meta.value) continue
    pending.add(key)
  }
  if (pending.size > 0 && timer === undefined) {
    timer = window.setTimeout(() => void flush(), 50)
  }
}

/**
 *
 * @param key
 */
function iconOf(key: string | null | undefined): string {
  return key ? (meta.value[key]?.icon ?? "") : ""
}

/**
 *
 * @param key
 * @param fallback
 */
function nameOf(key: string | null | undefined, fallback = ""): string {
  if (!key) return fallback
  return meta.value[key]?.name || fallback
}

/**
 *
 */
export function useAppMeta() {
  return { meta, loading, ensure, iconOf, nameOf }
}
