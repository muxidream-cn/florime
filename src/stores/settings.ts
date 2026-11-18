import { defineStore } from "pinia"
import { ref } from "vue"

import * as api from "@/api/commands"
import { applyTheme } from "@/lib/theme"

import type { Settings, SettingsPatch } from "@/types/models"

const FALLBACK: Settings = {
  trigger_mode: "poll",
  poll_ms: 1000,
  heartbeat_ms: 500,
  process_scan_ms: 2000,
  cleanup_remind_secs: 15,
  close_behavior: "minimize_to_tray",
  autostart: false,
  launch_as_admin: false,
  track_all_foreground: true,
  idle_pause_enabled: false,
  idle_pause_secs: 300,
  notify: true,
  sound: true,
  theme: "system",
  limit_reset_mode: "natural_day",
  whitelist: [],
  blacklist: [],
  extra_protected: [],
  retention_days: 180,
}

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<Settings>({ ...FALLBACK })
  const loading = ref(false)
  const saving = ref(false)
  const error = ref("")

  /**
   *
   */
  async function load(): Promise<void> {
    loading.value = true
    try {
      settings.value = await api.getSettings()
      applyTheme(settings.value.theme)
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  /**
   * 一次性提交多个字段；失败时回滚到服务端值。
   * @param next
   */
  async function patch(next: SettingsPatch): Promise<boolean> {
    saving.value = true
    error.value = ""
    const backup = { ...settings.value }
    settings.value = { ...settings.value, ...next } as Settings
    if (next.theme) applyTheme(next.theme)

    try {
      settings.value = await api.updateSettings(next)
      applyTheme(settings.value.theme)
      return true
    } catch (e) {
      settings.value = backup
      applyTheme(backup.theme)
      error.value = typeof e === "object" && e !== null && "message" in e ? String((e as { message: unknown }).message) : String(e)
      return false
    } finally {
      saving.value = false
    }
  }

  return { settings, loading, saving, error, load, patch }
})
