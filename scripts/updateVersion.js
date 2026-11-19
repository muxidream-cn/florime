// 统一更新应用版本：package.json / tauri.conf.json / appInfo.ts
// Cargo.toml 默认不更新：改它会触发 Rust 重新编译、令打包缓存失效；需要时用 --cargo 或向导中确认。
// 无参数且为交互终端时进入向导；带参数时保持命令行模式。
// Cargo.lock 由 cargo 在下次 check/build 时自动同步，不在此脚本写入。

import fs from "fs"
import { dirname, relative, resolve } from "path"
import { fileURLToPath } from "url"

import { InvalidArgumentError, program } from "commander"
import semver from "semver"

const __dirname = dirname(fileURLToPath(import.meta.url))
const rootDir = resolve(__dirname, "..")

const VERSION_PARTS = ["major", "minor", "patch"]
const VERSION_PATTERN = /^\d+\.\d+\.\d+$/
const TARGET_ID = "app"

const displayPath = (filePath) => relative(rootDir, filePath).replaceAll("\\", "/")

/**
 * 单端发布目标：默认写入应用版本号。
 * Cargo.toml 标为 optional：一般无依赖变更时不改它，避免 Rust 打包缓存失效。
 */
const VERSION_TARGET = {
  id: TARGET_ID,
  label: "莳间 florime",
  hint: "安装包 / updater / 设置页",
  writers: [
    { path: resolve(rootDir, "package.json"), kind: "json", key: "version" },
    { path: resolve(rootDir, "src-tauri/tauri.conf.json"), kind: "json", key: "version" },
    { path: resolve(rootDir, "src/lib/appInfo.ts"), kind: "ts-const", key: "APP_VERSION" },
  ],
}

const CARGO_WRITER = {
  path: resolve(rootDir, "src-tauri/Cargo.toml"),
  kind: "cargo",
  key: "version",
  optional: true,
}

/**
 * 按 option 决定是否把 Cargo.toml 纳入本次写入与预览
 * @param includeCargo
 */
function activeWriters(includeCargo) {
  return includeCargo ? [...VERSION_TARGET.writers, CARGO_WRITER] : VERSION_TARGET.writers
}

/**
 *
 * @param message
 */
function fail(message) {
  console.error(`[updateVersion] ❌ ${message}`)
  process.exit(1)
}

/**
 *
 * @param version
 * @param source
 */
function validateVersion(version, source) {
  if (typeof version !== "string" || !VERSION_PATTERN.test(version)) {
    fail(`版本号格式不合法: "${version}"（来自 ${source}），必须是 x.y.z 三段式数字`)
  }
  return version
}

/**
 *
 * @param value
 */
function parseVersionTag(value) {
  const clean = semver.clean(value)
  if (!clean || !VERSION_PATTERN.test(clean)) {
    throw new InvalidArgumentError(`版本号格式不合法: "${value}"，必须是 x.y.z 三段式数字（如 0.2.0）`)
  }
  return clean
}

program
  .version("1.0.0")
  .option("-v, --version-tag <version>", "指定目标版本号（优先级高于递增选项）", parseVersionTag)
  .option("--major", "递增主版本号（0.1.9 -> 1.0.0）")
  .option("--minor", "递增次版本号（0.1.9 -> 0.2.0）")
  .option("--patch", "递增修订号（0.1.9 -> 0.1.10），不传递增选项时的默认行为")
  .option("--cargo", "同时更新 Cargo.toml 版本（会使 Rust 打包缓存失效，默认跳过）")
  .option("--dry-run", "仅预览将写入的版本号，不修改任何文件")
  .parse(process.argv)

const options = program.opts()

/**
 * 读取某个 writer 当前值，文件缺失或字段不存在返回 undefined
 * @param writer
 */
function readWriterValue(writer) {
  if (!fs.existsSync(writer.path)) return undefined
  const text = fs.readFileSync(writer.path, "utf-8")

  if (writer.kind === "json") {
    const match = text.match(new RegExp(`^\\s*"${writer.key}":\\s*"([^"]+)"`, "m"))
    return match?.[1]
  }
  if (writer.kind === "cargo") {
    const match = text.match(/^version = "([^"]+)"/m)
    return match?.[1]
  }
  const match = text.match(new RegExp(`export const ${writer.key} = "([^"]+)"`))
  return match?.[1]
}

/**
 * 写出单个 writer：按文件类型只替换目标字段所在行，保留其余内容与换行风格
 * @param writer
 * @param version
 * @param dryRun
 */
function writeWriter(writer, version, dryRun) {
  if (!fs.existsSync(writer.path)) fail(`未找到文件: ${displayPath(writer.path)}`)

  const original = fs.readFileSync(writer.path, "utf-8")
  const bom = original.startsWith("\uFEFF") ? "\uFEFF" : ""
  const body = bom ? original.slice(1) : original
  const newline = body.includes("\r\n") ? "\r\n" : "\n"
  const lines = body.split(/\r?\n/)

  const { pattern, replacement } =
    writer.kind === "json"
      ? {
          pattern: new RegExp(`^(\\s*"${writer.key}":\\s*)"[^"]+"`),
          replacement: (line) => line.replace(pattern, `$1"${version}"`),
        }
      : writer.kind === "cargo"
        ? { pattern: /^version = "[^"]+"/, replacement: () => `version = "${version}"` }
        : {
            pattern: new RegExp(`^export const ${writer.key} = "[^"]+"`),
            replacement: () => `export const ${writer.key} = "${version}"`,
          }

  let replaced = false
  const nextLines = lines.map((line) => {
    if (replaced || !pattern.test(line)) return line
    replaced = true
    return replacement(line)
  })

  if (!replaced) fail(`${displayPath(writer.path)} 中未找到 ${writer.key} 字段`)

  if (!dryRun) fs.writeFileSync(writer.path, bom + nextLines.join(newline), "utf-8")
}

/**
 *
 * @param writers
 */
function inspectTarget(writers) {
  const found = []
  for (const writer of writers) {
    const value = readWriterValue(writer)
    if (value !== undefined && String(value).trim() !== "") {
      found.push({ path: displayPath(writer.path), value: String(value) })
    }
  }
  const versions = [...new Set(found.map((item) => item.value))]
  return { found, versions, current: versions.length === 1 ? versions[0] : null }
}

/**
 *
 */
function resolveCliOptions() {
  const parts = VERSION_PARTS.filter((part) => options[part])
  if (parts.length > 1) fail(`--major / --minor / --patch 只能选一个，当前: ${parts.map((p) => `--${p}`).join(" ")}`)

  const part = parts[0]
  if (options.versionTag && part) console.warn(`[updateVersion] 已指定目标版本号，忽略递增选项 --${part}`)

  return {
    action: options.versionTag ? "custom" : (part ?? "patch"),
    customVersion: options.versionTag,
    includeCargo: Boolean(options.cargo),
    dryRun: Boolean(options.dryRun),
  }
}

/**
 *
 * @param inspection
 */
async function runWizard(inspection) {
  const p = await import("@clack/prompts")
  const guard = (value) => {
    if (p.isCancel(value)) {
      p.cancel("已取消，未写入任何文件")
      process.exit(0)
    }
    return value
  }

  p.intro("[updateVersion] 版本号发布向导")

  const action = guard(
    await p.select({
      message: "选择版本动作",
      options: [
        { value: "patch", label: "递增修订号 patch", hint: "修复发布，如 0.1.9 → 0.1.10" },
        { value: "minor", label: "递增次版本号 minor", hint: "功能发布，如 0.1.9 → 0.2.0" },
        { value: "major", label: "递增主版本号 major", hint: "大版本发布，如 0.1.9 → 1.0.0" },
        { value: "custom", label: "指定目标版本号", hint: "手动输入 x.y.z" },
      ],
      initialValue: "patch",
    })
  )

  let customVersion
  if (action === "custom") {
    const input = guard(
      await p.text({
        message: "输入目标版本号（x.y.z 三段式数字）",
        placeholder: "0.2.0",
        validate: (value) => {
          const cleaned = semver.clean(value ?? "")
          return cleaned && VERSION_PATTERN.test(cleaned) ? undefined : "必须是 x.y.z 三段式数字，如 0.2.0"
        },
      })
    )
    customVersion = parseVersionTag(input)
  }

  const includeCargo = guard(
    await p.confirm({
      message: "同时更新 src-tauri/Cargo.toml 版本？（会使 Rust 打包缓存失效，通常无需更新）",
      initialValue: false,
    })
  )
  const dryRun = guard(await p.confirm({ message: "仅预览而不写入文件？", initialValue: false }))
  return { action, customVersion, includeCargo, dryRun, wizard: p }
}

/**
 *
 */
async function main() {
  const hasCliArgs = process.argv.slice(2).length > 0
  const useWizard = !hasCliArgs && Boolean(process.stdin.isTTY)

  if (!hasCliArgs && !process.stdin.isTTY) {
    fail("非交互终端且未提供参数，请显式指定 --patch / --minor / --major / -v（可用 --dry-run 预览）")
  }

  // 一致性以默认三处为准；Cargo.toml 常故意落后，不参与校验
  const inspection = inspectTarget(VERSION_TARGET.writers)
  if (!inspection.current) {
    const detail = inspection.found.map((item) => `    ${item.path} = ${item.value}`).join("\n")
    fail(`版本记录不一致或未设置：\n${detail || "    未找到任何版本记录"}\n请先用 -v 指定目标版本号统一，或手动对齐后重试`)
  }
  validateVersion(inspection.current, "版本记录")

  const choices = useWizard ? await runWizard(inspection) : resolveCliOptions()
  const { action, customVersion, includeCargo, dryRun } = choices
  const writers = activeWriters(includeCargo)

  let nextVersion
  if (action === "custom") {
    nextVersion = customVersion
  } else {
    const inc = semver.inc(inspection.current, action)
    if (!inc) fail(`无法基于 ${inspection.current} 递增 ${action}`)
    nextVersion = inc
  }
  validateVersion(nextVersion, "目标版本号")

  const planText = `  ${VERSION_TARGET.label.padEnd(14, " ")} ${inspection.current} → ${nextVersion}`
  if (choices.wizard) {
    choices.wizard.note(planText, dryRun ? "预览（不会写入任何文件）" : "即将写入以下版本号")
    const answer = await choices.wizard.confirm({ message: dryRun ? "生成预览？" : "确认写入以上版本号？", initialValue: false })
    if (choices.wizard.isCancel(answer) || !answer) {
      choices.wizard.cancel("已取消，未写入任何文件")
      return
    }
  } else {
    console.log(`[updateVersion] 版本动作: ${action}`)
    console.log(`[updateVersion] ${dryRun ? "预览" : "写入"}计划:\n${planText}`)
  }

  for (const writer of writers) {
    writeWriter(writer, nextVersion, dryRun)
    console.log(`[updateVersion] ${displayPath(writer.path)} ${dryRun ? "将" : "已"}更新为: ${nextVersion}`)
  }

  const skipNote = includeCargo ? "" : "\n[updateVersion] 已跳过 src-tauri/Cargo.toml（保持 Rust 打包缓存）；如需更新可加 --cargo"
  console.log(
    dryRun
      ? `\n[updateVersion] 🔍 预览完成，未写入任何文件，共 ${writers.length} 处待更新${skipNote}`
      : `\n[updateVersion] ✅ 已完成 ${writers.length} 处版本号更新${skipNote}`
  )
  if (includeCargo) console.log("[updateVersion] 提示: Cargo.lock 会在下次 cargo check/build 时自动同步主包版本")

  if (choices.wizard) choices.wizard.outro(dryRun ? "预览完成" : "版本号更新完成")
}

await main()
