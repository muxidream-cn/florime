/**
 * 应用级信息常量。
 *
 * 关于页、更新页、侧栏共用这一份数据，版本号不再在组件里各自硬编码。
 * 外部链接 URL 目前留空占位，待官网/仓库地址确定后补充。
 */

export const APP_NAME = "莳间"
export const APP_NAME_EN = "florime"
/** 与 tauri.conf.json / Cargo.toml 的 version 保持一致 */
export const APP_VERSION = "0.1.0"
export const APP_DESCRIPTION = "Windows 专注时长管理工具"
export const APP_AUTHOR = "默默 (MoMo) <1224021291@qq.com>"
export const APP_LICENSE = "MIT"
export const APP_COPYRIGHT = "Copyright © 2026 默默 (MoMo). 保留所有权利."

export interface AboutLink {
  /** 展示名，如「项目地址」 */
  label: string
  /** 点击后打开的地址；为空时「浏览器打开」按钮置灰 */
  url: string
}

/** 关于页的外部链接，顺序即展示顺序；URL 缺失处留空，后续补充 */
export const APP_LINKS: AboutLink[] = [
  { label: "项目地址", url: "https://github.com/muxidream-cn/florime" },
  { label: "网站地址", url: "https://florime.muxidream.cn/" },
  { label: "隐私协议", url: "" },
]

/**
 * 打开外部链接。
 *
 * TODO: 本项目尚未引入 @tauri-apps/plugin-opener 的 JS 包、commands.ts 也未暴露
 * open_url，先用 window.open 兜底；接入 Tauri opener 后替换此处实现。
 * @param url
 */
export function openUrl(url: string): void {
  if (!url) return
  window.open(url, "_blank", "noopener,noreferrer")
}
