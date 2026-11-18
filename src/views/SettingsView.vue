<script setup lang="ts">
import { FolderOpen, ShieldCheck } from "@lucide/vue"
import { onMounted, ref } from "vue"
import { toast } from "vue-sonner"

import { dbPath, isAutostartEnabled, openDataDir, restartAsAdmin } from "@/api/commands"
import AboutSection from "@/components/settings/AboutSection.vue"
import UpdateSection from "@/components/settings/UpdateSection.vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { useSettingsSectionNav } from "@/composables/useSettingsSectionNav"
import { settingsAnchor } from "@/lib/settingsSections"
import { useMonitorStore } from "@/stores/monitor"
import { useSettingsStore } from "@/stores/settings"

import type { Settings, SettingsPatch } from "@/types/models"

const settingsStore = useSettingsStore()
const monitor = useMonitorStore()
const dbLocation = ref("")
/** 注册表里的真实自启状态：配置文件说「开」不代表真的写进去了 */
const autostartActual = ref(false)

/** 分区导航：滚动高亮 + ?section= 双向同步，锚点与侧栏子项同源 */
const pageRef = ref<HTMLElement | null>(null)
useSettingsSectionNav(pageRef)

onMounted(async () => {
  await settingsStore.load()
  await syncAutostart()
  try {
    dbLocation.value = await dbPath()
  } catch {
    dbLocation.value = ""
  }
})

/**
 *
 */
async function syncAutostart(): Promise<void> {
  try {
    autostartActual.value = await isAutostartEnabled()
  } catch {
    autostartActual.value = settingsStore.settings.autostart
  }
}

/**
 *
 * @param enabled
 */
async function setAutostart(enabled: boolean): Promise<void> {
  await set("autostart", enabled)
  // 不信任配置文件，回读系统真实状态
  await syncAutostart()
}

/**
 *
 * @param key
 * @param value
 */
async function set<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void> {
  const patch: SettingsPatch = { [key]: value } as SettingsPatch
  const ok = await settingsStore.patch(patch)
  if (!ok && settingsStore.error) toast.error(settingsStore.error)
}

/**
 *
 * @param key
 * @param raw
 */
function setNumber<K extends keyof Settings>(key: K, raw: string | number): void {
  const n = Number(raw)
  if (Number.isFinite(n) && n > 0) void set(key, n as Settings[K])
}

/**
 *
 */
async function openDir(): Promise<void> {
  try {
    await openDataDir()
  } catch (e) {
    toast.error("打开目录失败", { description: String(e) })
  }
}

/**
 *
 */
async function elevate(): Promise<void> {
  try {
    await restartAsAdmin()
  } catch (e) {
    toast.error("提权失败", { description: String(e) })
  }
}
</script>

<template>
  <div ref="pageRef" class="flex flex-col gap-4">
    <div>
      <h1 class="text-base font-semibold">设置</h1>
      <!-- 写入 settings.json -->
      <p class="text-muted-foreground text-xs">改动即时生效</p>
    </div>

    <Card :id="settingsAnchor('focus')">
      <CardHeader>
        <CardTitle>专注</CardTitle>
        <CardDescription class="text-xs"> 事件模式在窗口切换时立即响应；轮询模式按固定周期采样。两种都保留心跳兜底。 </CardDescription>
      </CardHeader>
      <CardContent class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">触发方式</div>
            <div class="text-muted-foreground text-xs">事件模式切换归属更准，轮询模式更省心</div>
          </div>
          <Select
            :model-value="settingsStore.settings.trigger_mode"
            @update:model-value="(v: unknown) => set('trigger_mode', v as Settings['trigger_mode'])"
          >
            <SelectTrigger class="h-8 w-40 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="poll">周期轮询</SelectItem>
              <SelectItem value="event">窗口事件</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">轮询周期（毫秒）</div>
            <div class="text-muted-foreground text-xs">最小 500，默认 1000</div>
          </div>
          <Input
            :model-value="settingsStore.settings.poll_ms"
            type="number"
            min="500"
            step="100"
            class="h-8 w-32 text-xs"
            @change="(e: Event) => setNumber('poll_ms', (e.target as HTMLInputElement).value)"
          />
        </div>

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">进程存在性扫描（毫秒）</div>
            <div class="text-muted-foreground text-xs">仅"存在即计时"规则需要</div>
          </div>
          <Input
            :model-value="settingsStore.settings.process_scan_ms"
            type="number"
            min="1000"
            step="500"
            class="h-8 w-32 text-xs"
            @change="(e: Event) => setNumber('process_scan_ms', (e.target as HTMLInputElement).value)"
          />
        </div>

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">清理失败提醒间隔（秒）</div>
            <div class="text-muted-foreground text-xs">宽限期满仍有残留进程时，每隔这么久复检并重弹提醒（5–60，默认 15）</div>
          </div>
          <Input
            :model-value="settingsStore.settings.cleanup_remind_secs"
            type="number"
            min="5"
            max="60"
            step="5"
            class="h-8 w-32 text-xs"
            @change="(e: Event) => setNumber('cleanup_remind_secs', (e.target as HTMLInputElement).value)"
          />
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">统计设备使用时长</div>
            <div class="text-muted-foreground text-xs">
              记录所有前台程序的使用时长，只统计、不占规则额度；暂停时也保存继续记录。自身进程与白/黑名单不记录。
            </div>
          </div>
          <Switch :model-value="settingsStore.settings.track_all_foreground" @update:model-value="(v: boolean) => set('track_all_foreground', v)" />
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('quota')">
      <CardHeader>
        <CardTitle>额度</CardTitle>
      </CardHeader>
      <CardContent class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">重置方式</div>
            <div class="text-muted-foreground text-xs">自然日：本地 00:00 归零；启动后累计：关掉重开才归零</div>
          </div>
          <Select
            :model-value="settingsStore.settings.limit_reset_mode"
            @update:model-value="(v: unknown) => set('limit_reset_mode', v as Settings['limit_reset_mode'])"
          >
            <SelectTrigger class="h-8 w-40 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="natural_day">自然日 00:00</SelectItem>
              <SelectItem value="since_launch">启动后累计</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('tray')">
      <CardHeader>
        <CardTitle>托盘与启动</CardTitle>
      </CardHeader>
      <CardContent class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">关闭窗口时</div>
            <div class="text-muted-foreground text-xs">选"最小化到托盘"才能一直在后台监督</div>
          </div>
          <Select
            :model-value="settingsStore.settings.close_behavior"
            @update:model-value="(v: unknown) => set('close_behavior', v as Settings['close_behavior'])"
          >
            <SelectTrigger class="h-8 w-40 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="minimize_to_tray">最小化到托盘</SelectItem>
              <SelectItem value="quit">直接退出</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">开机自启</div>
            <div class="text-muted-foreground text-xs">写入当前用户的启动项</div>
            <div v-if="settingsStore.settings.autostart !== autostartActual" class="text-destructive text-xs">
              配置与实际启动项不一致，切换一次可重新写入
            </div>
          </div>
          <Switch :model-value="autostartActual" @update:model-value="(v: boolean) => setAutostart(v)" />
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">默认以管理员启动</div>
            <div class="text-muted-foreground text-xs">下次启动生效。非管理员时请求一次 UAC 提权，拒绝后本次按普通权限运行；开机自启也会请求。</div>
          </div>
          <Switch :model-value="settingsStore.settings.launch_as_admin" @update:model-value="(v: boolean) => set('launch_as_admin', v)" />
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('reminder')">
      <CardHeader>
        <CardTitle>提醒</CardTitle>
      </CardHeader>
      <CardContent class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">系统通知</div>
            <div class="text-muted-foreground text-xs">dev 模式下通知可能不显示（AUMID 未注册），正式安装后才正常</div>
          </div>
          <Switch :model-value="settingsStore.settings.notify" @update:model-value="(v: boolean) => set('notify', v)" />
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div class="text-xs">提示音</div>
          <Switch :model-value="settingsStore.settings.sound" @update:model-value="(v: boolean) => set('sound', v)" />
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-xs">空闲自动暂停</div>
            <div class="text-muted-foreground text-xs">无键鼠输入超过阈值就停止计时。看视频会被误判，默认关闭</div>
          </div>
          <Switch :model-value="settingsStore.settings.idle_pause_enabled" @update:model-value="(v: boolean) => set('idle_pause_enabled', v)" />
        </div>

        <div v-if="settingsStore.settings.idle_pause_enabled" class="flex items-center justify-between gap-4">
          <div class="text-xs">空闲阈值（秒）</div>
          <Input
            :model-value="settingsStore.settings.idle_pause_secs"
            type="number"
            min="60"
            step="60"
            class="h-8 w-32 text-xs"
            @change="(e: Event) => setNumber('idle_pause_secs', (e.target as HTMLInputElement).value)"
          />
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('appearance')">
      <CardHeader>
        <CardTitle>外观</CardTitle>
      </CardHeader>
      <CardContent>
        <div class="flex items-center justify-between gap-4">
          <div class="text-xs">主题</div>
          <Select :model-value="settingsStore.settings.theme" @update:model-value="(v: unknown) => set('theme', v as Settings['theme'])">
            <SelectTrigger class="h-8 w-40 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">跟随系统</SelectItem>
              <SelectItem value="light">浅色</SelectItem>
              <SelectItem value="dark">深色</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('data')">
      <CardHeader>
        <CardTitle>数据</CardTitle>
        <CardDescription class="text-xs"> 明细保留 {{ settingsStore.settings.retention_days }} 天，聚合数据永久保留。 </CardDescription>
      </CardHeader>
      <CardContent class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div class="text-xs">明细保留天数</div>
          <Input
            :model-value="settingsStore.settings.retention_days"
            type="number"
            min="7"
            step="30"
            class="h-8 w-32 text-xs"
            @change="(e: Event) => setNumber('retention_days', (e.target as HTMLInputElement).value)"
          />
        </div>

        <Separator />

        <div class="flex items-center justify-between gap-4">
          <div class="min-w-0">
            <div class="text-xs">数据库位置</div>
            <div class="text-muted-foreground truncate font-mono text-xs">
              {{ dbLocation || "读取中…" }}
            </div>
          </div>
          <Button variant="outline" size="sm" @click="openDir">
            <FolderOpen data-icon="inline-start" />
            打开目录
          </Button>
        </div>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('permission')">
      <CardHeader>
        <CardTitle>权限</CardTitle>
        <CardDescription class="text-xs"> 处置高完整性（管理员权限运行）的程序需要本程序也提权。平时不需要。 </CardDescription>
      </CardHeader>
      <CardContent class="flex items-center justify-between gap-4">
        <Badge
          variant="outline"
          :class="
            monitor.elevated
              ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
              : 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400'
          "
        >
          {{ monitor.elevated ? "当前已是管理员" : "普通权限" }}
        </Badge>
        <Button v-if="!monitor.elevated" variant="outline" size="sm" @click="elevate">
          <ShieldCheck data-icon="inline-start" />
          以管理员重启
        </Button>
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('update')">
      <CardHeader>
        <CardTitle>更新</CardTitle>
        <CardDescription class="text-xs">更新检查与下载暂未实现</CardDescription>
      </CardHeader>
      <CardContent>
        <UpdateSection />
      </CardContent>
    </Card>

    <Card :id="settingsAnchor('about')">
      <CardHeader>
        <CardTitle>关于</CardTitle>
      </CardHeader>
      <CardContent>
        <AboutSection />
      </CardContent>
    </Card>
  </div>
</template>
