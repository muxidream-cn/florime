import { ref } from "vue"

interface CardSortOptions {
  /** 当前完整顺序（含被过滤掉的条目），drop 时以它为基准，保证不丢 id */
  ids: () => string[]
  /** 提交新顺序；返回 Promise 时失败由调用方（store）回滚 */
  commit: (ids: string[]) => void | Promise<void>
}

/**
 * 卡片拖拽排序（原生 HTML5 Drag & Drop）。
 *
 * 只有手柄（`handleProps`）是可拖元素，卡片本体只接收放置事件，
 * 因此拖开关、按钮、图标都不会误触发整卡拖动。
 *
 * 拖动过程中不重排 DOM —— 移动被拖元素会中断 drag 事件；
 * 只记录悬停目标做高亮，松手（drop）时一次性算出新顺序并提交。
 * @param options
 */
export function useCardSort(options: CardSortOptions) {
  const draggingId = ref<string | null>(null)
  const overId = ref<string | null>(null)

  /**
   *
   */
  function reset(): void {
    draggingId.value = null
    overId.value = null
  }

  /**
   *
   * @param id
   */
  function isDragging(id: string): boolean {
    return draggingId.value === id
  }

  /**
   *
   * @param id
   */
  function isOver(id: string): boolean {
    return overId.value === id && draggingId.value !== id
  }

  /**
   *
   * @param e
   * @param id
   */
  function onHandleDragStart(e: DragEvent, id: string): void {
    const handle = e.currentTarget as HTMLElement | null
    const card = handle?.closest("[data-sortable-card]")
    // 默认拖动图是那个小图标，换成整卡幽灵更直观
    if (card instanceof HTMLElement) e.dataTransfer?.setDragImage(card, 16, 16)

    draggingId.value = id
    overId.value = null
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move"
      e.dataTransfer.setData("text/plain", id)
    }
  }

  /**
   *
   * @param e
   * @param id
   */
  function onCardDragOver(e: DragEvent, id: string): void {
    if (!draggingId.value) return
    // 不 preventDefault 就永远不允许放置
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move"
    if (draggingId.value !== id) overId.value = id
  }

  /**
   *
   * @param id
   */
  function onCardDragLeave(id: string): void {
    if (overId.value === id) overId.value = null
  }

  /**
   *
   * @param e
   * @param targetId
   */
  function onCardDrop(e: DragEvent, targetId: string): void {
    e.preventDefault()
    const from = draggingId.value
    reset()
    if (!from || from === targetId) return

    const all = options.ids()
    const fromIndex = all.indexOf(from)
    const targetIndex = all.indexOf(targetId)
    if (fromIndex < 0 || targetIndex < 0) return

    const next = all.filter((id) => id !== from)
    const anchor = next.indexOf(targetId)
    // 往下拖插到目标之后，往上拖插到目标之前
    next.splice(fromIndex < targetIndex ? anchor + 1 : anchor, 0, from)
    void options.commit(next)
  }

  /**
   * 卡片本体：作为放置目标
   * @param id
   */
  function cardProps(id: string): Record<string, unknown> {
    return {
      "data-sortable-card": "",
      onDragover: (e: DragEvent) => onCardDragOver(e, id),
      onDragleave: () => onCardDragLeave(id),
      onDrop: (e: DragEvent) => onCardDrop(e, id),
    }
  }

  /**
   * 拖动手柄：唯一的可拖元素
   * @param id
   */
  function handleProps(id: string): Record<string, unknown> {
    return {
      draggable: true,
      title: "拖动排序",
      onDragstart: (e: DragEvent) => onHandleDragStart(e, id),
      onDragend: reset,
    }
  }

  return { draggingId, overId, isDragging, isOver, cardProps, handleProps }
}
