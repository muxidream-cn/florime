<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

const props = withDefaults(
  defineProps<{
    text: string
    /** 尾部保留的字符数：路径的文件名通常在这里 */
    tailChars?: number
    /** 头部至少保留的字符数（盘符 / 一级目录） */
    minHeadChars?: number
  }>(),
  { tailChars: 14, minHeadChars: 4 }
)

const ELLIPSIS = "…"

const host = ref<HTMLSpanElement | null>(null)
const width = ref(0)
let observer: ResizeObserver | null = null
let measureCtx: CanvasRenderingContext2D | null = null

/**
 *
 * @param text
 */
function textWidth(text: string): number {
  if (!host.value) return 0
  if (!measureCtx) {
    measureCtx = document.createElement("canvas").getContext("2d")
  }
  if (!measureCtx) return 0
  const style = getComputedStyle(host.value)
  // 用元素自身的字体量宽度，避免等宽字体与比例字体混用后长度对不上
  measureCtx.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
  return measureCtx.measureText(text).width
}

/**
 *
 * @param text
 * @param max
 */
function shrink(text: string, max: number): string {
  const tail = Math.min(props.tailChars, Math.max(0, text.length - props.minHeadChars - 1))
  for (let head = text.length - tail - 1; head >= props.minHeadChars; head--) {
    const candidate = `${text.slice(0, head)}${ELLIPSIS}${text.slice(text.length - tail)}`
    if (textWidth(candidate) <= max) return candidate
  }
  // 极窄容器：逐步缩短尾部，至少保留文件名后缀
  for (let t = tail; t > 2; t--) {
    const candidate = `${text.slice(0, props.minHeadChars)}${ELLIPSIS}${text.slice(text.length - t)}`
    if (textWidth(candidate) <= max) return candidate
  }
  return `${text.slice(0, props.minHeadChars)}${ELLIPSIS}`
}

const display = computed(() => {
  const text = props.text
  if (!text) return ""
  // 容器还没量到宽度时先原样输出，交给 CSS 兜底
  if (!width.value) return text
  if (textWidth(text) <= width.value) return text
  return shrink(text, width.value)
})

/**
 *
 */
function syncWidth(): void {
  width.value = host.value?.clientWidth ?? 0
}

onMounted(() => {
  syncWidth()
  if (!host.value || typeof ResizeObserver === "undefined") return
  observer = new ResizeObserver(syncWidth)
  observer.observe(host.value)
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
})
</script>

<template>
  <span ref="host" class="mt-1 block min-w-0 overflow-hidden whitespace-nowrap" :title="text">
    {{ display }}
  </span>
</template>
