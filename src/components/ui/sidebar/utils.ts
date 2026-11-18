import type { ComputedRef, Ref } from "vue";
import { createContext } from "reka-ui";

/** 展开态宽度：与原先固定 208px 的侧栏相比略宽，容得下两级菜单缩进 */
export const SIDEBAR_WIDTH = "16rem";
/** 收起态宽度：只留图标轨道 */
export const SIDEBAR_WIDTH_ICON = "3rem";
export const SIDEBAR_KEYBOARD_SHORTCUT = "b";
/** 展开态持久化键：桌面应用没有需要兼容的 Web 端，用 localStorage 比 cookie 直白 */
export const SIDEBAR_STORAGE_KEY = "florime:sidebar-open";

/**
 * 侧栏上下文。
 *
 * 相比参考项目去掉了移动端字段（isMobile / openMobile / setOpenMobile）：
 * 本项目是桌面窗口，最小宽度 1000px，不存在抽屉形态。
 */
export const [useSidebar, provideSidebarContext] = createContext<{
  state: ComputedRef<"expanded" | "collapsed">;
  open: Ref<boolean>;
  setOpen: (value: boolean) => void;
  toggleSidebar: () => void;
}>("Sidebar");
