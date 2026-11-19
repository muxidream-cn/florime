# 莳间 florime

Windows 桌面专注管理工具。识别前台窗口进程，按规则累计使用时长，超限提醒并可配置关闭进程。全部数据本机存储，无后端。

## 安装

需要 Node.js、pnpm 与 Rust 工具链（Tauri 2 开发环境，详见 [Tauri 前置要求](https://v2.tauri.app/start/prerequisites/)）。

```bash
git clone git@github.com:muxidream-cn/florime.git
cd florime
pnpm install
```

## 开发

```bash
pnpm app:dev        # 启动开发环境（前端 + Tauri 窗口）
pnpm app:build      # 构建 Windows NSIS 安装包
```

单独运行前端或仅做类型检查：

```bash
pnpm dev            # 仅启动 Vite 前端
pnpm build          # vue-tsc 类型检查 + Vite 构建
```

## 自动更新

应用内置 Tauri
updater。设置页「更新」卡片可手动检查新版本，检测到更新后弹出对话框显示下载进度，支持一键下载、安装并重启；「启动时检查更新」为本地偏好（存于浏览器
`localStorage`，不随规则库同步），开启后每次启动延迟检查一次。

更新清单由 CI 随 Release 发布，客户端从 `https://github.com/muxidream-cn/florime/releases/latest/download/latest.json` 拉取。发布流程：

1. 用 `pnpm updateVersion` 统一升级版本号（交互向导，或 `-v 0.2.0` / `--major` / `--minor` / `--patch`，可加 `--dry-run` 预览），会同步更新
   `package.json`、`src-tauri/tauri.conf.json` 与 `src/lib/appInfo.ts` 三处。`src-tauri/Cargo.toml` 默认跳过以保持 Rust
   打包缓存，仅在依赖确需变更时加 `--cargo`（更新后 `Cargo.lock` 会在下次 cargo 构建时自动同步）；然后打 `v<版本号>` tag 并推送。
2. CI 构建 NSIS 安装包，用仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`（及可选的 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）签名，产出 `.exe` 与 `.exe.sig`。
3. release job 校验 tag 与版本一致、签名非空，运行 `scripts/generate-latest-json.mjs` 生成 `latest.json`，随安装包一起挂到该 tag 的 Release。

配置中的 `pubkey` 必须与云端私钥配对，否则客户端校验签名会失败。`latest.json` 中的下载地址固定指向当前仓库的 Release asset，文件名必须为
`florime_<版本>_x64-setup.exe`。

## 技术栈

- **Tauri 2** + **Vue 3** + **TypeScript** + **Vite 8**
- **shadcn-vue**（style `reka-vega`）+ **Tailwind CSS v4**（CSS-first，无 tailwind.config）
- Rust 侧：`windows` crate 直调 Win32、`rusqlite`（bundled）持久化

## 目录结构

| 路径                                       | 说明                                                              |
| ------------------------------------------ | ----------------------------------------------------------------- |
| `src/`                                     | 前端 Vue 应用源码                                                 |
| `src/components/`                          | 业务组件与 shadcn-vue UI 组件                                     |
| `src/views/`                               | 页面视图                                                          |
| `src/stores/`                              | Pinia 状态                                                        |
| `src/composables/`                         | 组合式函数                                                        |
| `src/assets/css/`                          | 全局样式与主题 token                                              |
| `src-tauri/src/`                           | Rust 后端模块（`monitor/` 计时、`win/` Win32 交互、`db/` 持久化） |
| `src-tauri/icons/`                         | 应用图标                                                          |
| `src-tauri/tauri.conf.json`                | Tauri 应用与打包配置                                              |
| `src-tauri/tauri.microsoftstore.conf.json` | Microsoft Store 打包专用配置                                      |
| `scripts/build-microsoft-store.ps1`        | MSIXBundle 构建脚本                                               |
| `.github/workflows/`                       | GitHub Actions 工作流                                             |

## Microsoft Store 打包

`app:build` 产出 NSIS 安装包；Microsoft Store 上传用的 MSIXBundle 由 Windows SDK 的 `MakeAppx.exe` 单独封装，脚本会构建 x64 +
arm64 两个架构、生成 manifest 并自检语言/发布者/图标。前置要求见参考项目的打包文档；身份不写死在代码里，运行前需设置三个环境变量（CI 中由同名仓库 Variables 注入）：

| 环境变量                      | 说明                                        |
| ----------------------------- | ------------------------------------------- |
| `MSIX_IDENTITY_NAME`          | Partner Center 分配的 Package/Identity/Name |
| `MSIX_PUBLISHER`              | Partner Center 分配的 Publisher（`CN=...`） |
| `MSIX_PUBLISHER_DISPLAY_NAME` | Partner Center 分配的发布者显示名           |

本地构建：

```powershell
$env:MSIX_IDENTITY_NAME = "..."
$env:MSIX_PUBLISHER = "CN=..."
$env:MSIX_PUBLISHER_DISPLAY_NAME = "..."
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-microsoft-store.ps1
```

产物位于 `src-tauri/target/microsoft-store/florime_<四段版本>_x64_arm64.msixbundle`。

## 脚本说明

| 脚本             | 说明                                              |
| ---------------- | ------------------------------------------------- |
| `dev`            | 启动 Vite 前端开发服务器                          |
| `build`          | 类型检查并构建前端                                |
| `preview`        | 预览前端构建产物                                  |
| `app:dev`        | 启动 Tauri 开发环境                               |
| `app:build`      | 构建 Windows 安装包                               |
| `updateVersion`  | 统一升级三处应用版本号（Cargo.toml 需加 --cargo） |
| `check`          | ESLint 检查                                       |
| `check:prettier` | Prettier 格式检查                                 |
| `lint`           | ESLint 检查并自动修复                             |
| `lint:prettier`  | Prettier 格式化                                   |
| `commit`         | 暂存区检查并进入引导式提交（commitlint + husky）  |

提交信息遵循 commitlint 规范（引导式提交可用 `pnpm commit`）。

## 架构要点

计时与进程检测**全部在 Rust 后台线程**（`src-tauri/src/monitor/`）。前端只显示 `focus://tick`
推送的值，不本地累加 —— 主窗口最小化或被遮挡时 WebView2 会把定时器节流到 1 次/分钟，关到托盘后前端根本不存在。

| 模块                    | 职责                                                                                          |
| ----------------------- | --------------------------------------------------------------------------------------------- |
| `win/foreground.rs`     | `GetForegroundWindow` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`，含三态降级 |
| `win/procs.rs`          | Toolhelp32 进程快照、进程树展开、同名根进程定位                                               |
| `win/protected.rs`      | 系统关键进程保护表 + 自身保护集                                                               |
| `monitor/integrator.rs` | `Instant` 积分计时、休眠间隙判定、本地 00:00 切分                                             |
| `monitor/limits.rs`     | 每规则限额状态机（Normal → Warning → Enforced）                                               |
| `db/`                   | SQLite（WAL），批量写入 + rollup 聚合                                                         |

规则与设置存 `%APPDATA%\com.muxidream.florime\`（JSON，可手改），统计库在同目录 `focus.db`。

## 已知限制

1. **退出应用即失效**。没有看门狗/服务，任务管理器结束进程就能绕过。要提升只能做 Windows 服务 + 独立守护进程，成本翻倍。
2. **高完整性 / PPL 保护进程**只能识别到 PID 或完全拿不到名字，也无法 `TerminateProcess`。需要以管理员身份运行才能部分改善（设置页有入口）。
3. **安全桌面**（UAC 弹窗、锁屏）与其他用户会话收不到前台事件；这类时段不计时。
4. **UWP / 商店应用**前台窗口属于 `ApplicationFrameHost.exe`，靠枚举子窗口反查真实 PID；查不到时标 `unknown` 且不计时（宁可漏计，不可错罚）。
5. `dev` 模式下系统通知常不显示（AUMID 未注册），正式安装后才正常。置顶提醒窗是主信号，通知只是补充。

## 匹配与安全

- 默认按**完整路径**匹配。同名 exe 太多（`python.exe`、`node.exe`），按名称匹配容易误伤，UI 里只作为高级选项。
- 处置前必过五道闸：自身/祖先 → pid 0/4 → 系统保护表 → 白名单 → 规则允许动作。展开进程树后逐个再校验。
- `%WINDIR%` 下的进程、自身进程、全部 `msedgewebview2.exe` 永不可杀。

## 许可证

本项目采用 [MIT](./LICENSE) 许可证，详见 [LICENSE](./LICENSE)。
