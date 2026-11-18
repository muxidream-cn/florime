import { useEventListener } from "@vueuse/core"
import { nextTick, onMounted, ref, watch, type Ref } from "vue"
import { useRoute, useRouter } from "vue-router"

import { settingsAnchor, settingsSections } from "@/lib/settingsSections"

/** 判定线偏移：让分区标题落在可视区顶部留白之下 */
const OFFSET = 16

/**
 * 设置页的分区导航：滚动高亮 + 地址同步 + 点击滚动。
 *
 * 与参考项目的差异：本项目页面本身不滚动，滚动容器是 AppShell 的 `main`，
 * 所以这里通过 `closest("main")` 现取；滚动锚点由 `settingsSections` 统一生成。
 * @param pageRef
 */
export function useSettingsSectionNav(pageRef: Ref<HTMLElement | null>): void {
  const route = useRoute()
  const router = useRouter()

  const scrollRef = ref<HTMLElement | null>(null)
  /** 程序化滚动期间的静音截止时间：避免平滑滚动途中反复改写地址 */
  let muteUntil = 0

  /**
   *
   * @param id
   */
  function sectionEl(id: string): HTMLElement | null {
    return pageRef.value?.querySelector<HTMLElement>(`#${CSS.escape(settingsAnchor(id))}`) ?? null
  }

  /**
   *
   * @param id
   * @param behavior
   */
  function scrollToSection(id: string, behavior: ScrollBehavior = "smooth"): void {
    const box = scrollRef.value
    const el = sectionEl(id)
    if (!box || !el) return
    if (behavior === "smooth") muteUntil = Date.now() + 700
    const top = el.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop - OFFSET
    box.scrollTo({ top: Math.max(0, top), behavior })
  }

  /** 当前视口命中的分区：取最后一个已滚过判定线的锚点 */
  function activeFromScroll(): string {
    const box = scrollRef.value
    if (!box) return ""
    let current = settingsSections[0].id
    for (const section of settingsSections) {
      const el = sectionEl(section.id)
      if (!el) continue
      const top = el.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop
      if (top <= box.scrollTop + OFFSET) current = section.id
    }
    // 滚到底时强制点亮最后一项：最后一张卡较矮时否则会一直亮着上一项
    if (box.scrollTop + box.clientHeight >= box.scrollHeight - 2) {
      current = settingsSections[settingsSections.length - 1].id
    }
    return current
  }

  /**
   * 由页面自己写回地址的分区集合：用来忽略自己触发的 query 变化。
   * 用集合而不是单个值——快速连续滚动时可能同时有多次 replace 在途。
   */
  const selfWritten = new Set<string>()

  /**
   * 只在设置页生效；用 replace 不污染前进后退栈，也不会触发编辑页的脏数据守卫
   * @param id
   */
  function syncQuery(id: string): void {
    if (!id || route.name !== "settings") return
    if (String(route.query.section ?? "") === id) return
    selfWritten.add(id)
    void router.replace({ query: { ...route.query, section: id } })
  }

  let frame = 0
  /**
   *
   */
  function onScroll(): void {
    if (Date.now() < muteUntil || frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      syncQuery(activeFromScroll())
    })
  }

  useEventListener(scrollRef, "scroll", onScroll, { passive: true })

  // 点侧栏子项 / 前进后退：地址变了就把对应分区滚进视野
  watch(
    () => route.query.section,
    (value) => {
      const id = String(value ?? "")
      // 忽略滚动自己写回的地址，否则会把用户刚滚到的位置再拉回分区顶部
      if (id && selfWritten.has(id)) {
        selfWritten.delete(id)
        return
      }
      if (route.name !== "settings" || !id) return
      if (!sectionEl(id)) return
      scrollToSection(id)
    }
  )

  onMounted(async () => {
    scrollRef.value = pageRef.value?.closest("main") ?? null
    await nextTick()
    const id = String(route.query.section ?? "")
    if (id) scrollToSection(id, "auto")
    else syncQuery(activeFromScroll())
  })
}
