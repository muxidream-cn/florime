<script setup lang="ts">
import { shallowRef } from "vue"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"

import changelogRaw from "../../CHANGELOG.md?raw"

/** 更新日志条目：标题或列表项，按行轻量解析，避免引入 Markdown 渲染库与 v-html */
interface ChangelogEntry {
  kind: "heading" | "item" | "text"
  text: string
  level?: number
}

/**
 *
 * @param raw
 */
function parseChangelog(raw: string): ChangelogEntry[] {
  return raw
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.trim() !== "")
    .map((line): ChangelogEntry => {
      const heading = line.match(/^(#{1,3})\s+(.*)$/)
      if (heading) return { kind: "heading", level: heading[1].length, text: heading[2] }
      const item = line.match(/^[-*]\s+(.*)$/)
      if (item) return { kind: "item", text: item[1] }
      return { kind: "text", text: line }
    })
}

const entries = shallowRef<ChangelogEntry[]>(parseChangelog(changelogRaw))
const open = shallowRef(false)
</script>

<template>
  <Button variant="link" size="sm" class="text-muted-foreground h-auto p-0 text-xs" @click="open = true"> 查看更新日志 </Button>

  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>更新日志</DialogTitle>
        <DialogDescription class="text-xs">各版本的变更记录</DialogDescription>
      </DialogHeader>

      <div class="max-h-[60vh] overflow-y-auto pr-1">
        <template v-for="(entry, index) in entries" :key="index">
          <h3 v-if="entry.kind === 'heading' && entry.level === 1" class="mb-2 text-base font-semibold">
            {{ entry.text }}
          </h3>
          <h4 v-else-if="entry.kind === 'heading' && entry.level === 2" class="mt-4 mb-1.5 text-sm font-semibold first:mt-0">
            {{ entry.text }}
          </h4>
          <h5 v-else-if="entry.kind === 'heading'" class="mt-3 mb-1 text-xs font-semibold">{{ entry.text }}</h5>
          <div v-else-if="entry.kind === 'item'" class="text-muted-foreground flex gap-2 py-0.5 text-xs">
            <span class="mt-1.5 size-1 shrink-0 rounded-full bg-current" />
            <span class="whitespace-pre-wrap">{{ entry.text }}</span>
          </div>
          <p v-else class="text-muted-foreground py-0.5 text-xs whitespace-pre-wrap">{{ entry.text }}</p>
        </template>
      </div>

      <DialogFooter>
        <Button variant="outline" @click="open = false">关闭</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
