import { readFileSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"

/**
 * 生成 Tauri updater 静态清单 latest.json。
 * 用法：node scripts/generate-latest-json.mjs <version> <signatureFile> <outputFile>
 * 签名文本来自 tauri build 产出的 .sig，必须非空；下载 URL 固定指向本仓库 Release。
 */
const [, , version, signatureFile, outputFile] = process.argv

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error(`版本号无效：${version}（应为 x.y.z）`)
}
if (!signatureFile || !outputFile) {
  throw new Error("用法：node scripts/generate-latest-json.mjs <version> <signatureFile> <outputFile>")
}

const signature = readFileSync(resolve(signatureFile), "utf8").trim()
if (!signature) {
  throw new Error("签名文件为空，无法生成 latest.json")
}

const downloadUrl = `https://github.com/muxidream-cn/florime/releases/download/v${version}/florime_${version}_x64-setup.exe`

const manifest = {
  version,
  notes: "详见 Release 说明",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: downloadUrl,
    },
  },
}

writeFileSync(resolve(outputFile), `${JSON.stringify(manifest, null, 2)}\n`, "utf8")
console.log(`已生成 ${outputFile}`)
console.log(`版本 ${version}，签名长度 ${signature.length}，下载地址 ${downloadUrl}`)
