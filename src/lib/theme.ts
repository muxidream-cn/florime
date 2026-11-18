import type { ThemeMode } from "@/types/models"

let mediaQuery: MediaQueryList | null = null
let currentMode: ThemeMode = "system"

/**
 *
 * @param mode
 */
function resolve(mode: ThemeMode): boolean {
  if (mode === "dark") return true
  if (mode === "light") return false
  return window.matchMedia("(prefers-color-scheme: dark)").matches
}

/**
 *
 */
function paint(): void {
  document.documentElement.classList.toggle("dark", resolve(currentMode))
}

/**
 * 应用主题。`system` 会跟随系统变化实时切换。
 * @param mode
 */
export function applyTheme(mode: ThemeMode): void {
  currentMode = mode

  if (!mediaQuery) {
    mediaQuery = window.matchMedia("(prefers-color-scheme: dark)")
    mediaQuery.addEventListener("change", () => {
      if (currentMode === "system") paint()
    })
  }
  paint()
}
