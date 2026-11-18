export interface SettingsSection {
  /** 既是查询参数值，也是页面锚点后缀（#settings-focus） */
  id: string
  title: string
}

/**
 * 设置页分区清单。
 *
 * 侧栏二级菜单与设置页卡片锚点共用这一份数据：新增分区只改这里 + 页面加一张卡，
 * 不需要再去侧栏补一遍菜单项。
 */
export const settingsSections: SettingsSection[] = [
  { id: "focus", title: "专注" },
  { id: "quota", title: "额度" },
  { id: "tray", title: "托盘与启动" },
  { id: "reminder", title: "提醒" },
  { id: "appearance", title: "外观" },
  { id: "data", title: "数据" },
  { id: "permission", title: "权限" },
  { id: "update", title: "更新" },
  { id: "about", title: "关于" },
]

/**
 * 卡片锚点 id：与侧栏子项的 section 一一对应
 * @param id
 */
export function settingsAnchor(id: string): string {
  return `settings-${id}`
}
