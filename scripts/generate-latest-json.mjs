import { readFileSync, statSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const __dirname = dirname(fileURLToPath(import.meta.url))

/**
 * 生成 Tauri updater 静态清单 latest.json。
 * 用法：node scripts/generate-latest-json.mjs <version> <signatureFile> <outputFile> [exeFile]
 * 签名文本来自 tauri build 产出的 .sig，必须非空；下载 URL 固定指向本仓库 Release。
 * notes / force_update 从仓库根 CHANGELOG.md 对应版本段落提取；exeFile 省略时由签名文件推导。
 */
const [, , version, signatureFile, outputFile, exeFileArg] = process.argv

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error(`版本号无效：${version}（应为 x.y.z）`)
}
if (!signatureFile || !outputFile) {
  throw new Error("用法：node scripts/generate-latest-json.mjs <version> <signatureFile> <outputFile> [exeFile]")
}

const signature = readFileSync(resolve(signatureFile), "utf8").trim()
if (!signature) {
  throw new Error("签名文件为空，无法生成 latest.json")
}

/**
 * 解析 CHANGELOG 中指定版本段落，返回更新说明与强制更新标记。
 * 支持 "## v1.2.3"、"## [1.2.3] - 日期" 两类标题，提取到下一个二级标题前。
 * 段落内单独一行 `force: true`（可带 "- " 前缀）表示该版本强制更新，会从说明中剔除。
 * @param text CHANGELOG.md 全文
 * @param targetVersion 目标版本号
 * @returns 包含 notes 与 forceUpdate 的结果对象
 */
export function parseChangelogSection(text, targetVersion) {
  const lines = text.split(/\r?\n/)
  const headingPattern = /^##\s+(?:v|\[)?(\d+\.\d+\.\d+)\]?(?:\s|$)/i
  const anyHeadingPattern = /^##\s/
  const forcePattern = /^\s*(?:[-*]\s*)?force\s*:\s*true\s*$/i

  let capturing = false
  let forceUpdate = false
  const collected = []

  for (const line of lines) {
    const match = line.match(headingPattern)
    if (match) {
      if (capturing) break
      if (match[1] === targetVersion) {
        capturing = true
        continue
      }
    } else if (capturing && anyHeadingPattern.test(line)) {
      break
    }
    if (capturing) {
      if (forcePattern.test(line)) {
        forceUpdate = true
        continue
      }
      collected.push(line)
    }
  }

  return { notes: collected.join("\n").trim(), forceUpdate }
}

/**
 * 仅取更新说明，供外部复用
 * @param text CHANGELOG.md 全文
 * @param targetVersion 目标版本号
 */
export function extractChangelogNotes(text, targetVersion) {
  return parseChangelogSection(text, targetVersion).notes
}

/** 从 CHANGELOG 解析目标版本的说明与强制标记，失败时降级为默认说明 */
function resolveChangelogData() {
  const changelogPath = resolve(__dirname, "..", "CHANGELOG.md")
  try {
    const changelog = readFileSync(changelogPath, "utf8")
    const { notes, forceUpdate } = parseChangelogSection(changelog, version)
    if (notes) {
      console.log(`已从 CHANGELOG.md 提取 ${version} 的更新说明（${notes.length} 字符）`)
    } else {
      console.warn(`::warning::CHANGELOG.md 中 v${version} 段落无说明内容，notes 使用默认说明`)
    }
    if (forceUpdate) console.log(`版本 ${version} 标记为强制更新`)
    return { notes: notes || "详见 Release 说明", forceUpdate }
  } catch (error) {
    console.warn(`::warning::读取 CHANGELOG.md 失败：${error instanceof Error ? error.message : String(error)}`)
    return { notes: "详见 Release 说明", forceUpdate: false }
  }
}

/** 读取安装包字节大小；优先用显式 exeFile 参数，否则由签名文件路径推导 */
function resolveSize() {
  const exeFile = exeFileArg || signatureFile.replace(/\.sig$/i, "")
  try {
    const { size } = statSync(resolve(exeFile))
    if (!Number.isSafeInteger(size) || size <= 0) throw new Error(`无效文件大小 ${size}`)
    console.log(`已读取安装包大小：${size} 字节（${exeFile}）`)
    return size
  } catch (error) {
    console.warn(`::warning::读取安装包大小失败：${error instanceof Error ? error.message : String(error)}`)
    return 0
  }
}

const changelogData = resolveChangelogData()
const downloadUrl = `https://github.com/muxidream-cn/florime/releases/download/v${version}/florime_${version}_x64-setup.exe`

const manifest = {
  version,
  notes: changelogData.notes,
  pub_date: new Date().toISOString(),
  size: resolveSize(),
  force_update: changelogData.forceUpdate,
  platforms: {
    "windows-x86_64": {
      signature,
      url: downloadUrl,
    },
  },
}

writeFileSync(resolve(outputFile), `${JSON.stringify(manifest, null, 2)}\n`, "utf8")
console.log(`已生成 ${outputFile}`)
console.log(`版本 ${version}，签名长度 ${signature.length}，强制更新 ${manifest.force_update}，下载地址 ${downloadUrl}`)
