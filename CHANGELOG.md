# 更新日志

本文件记录莳间 florime 各版本的可交付变更。发布时 CI 会按目标版本号提取对应段落，写入 `latest.json` 的 `notes`，在应用内更新对话框中展示。

## Unreleased

## v0.1.3

- 重做检查更新弹窗：展示最新版本/当前版本对比、发布时间与包体大小
- 更新说明改为可滚动区域，支持多行文本与强制更新醒目提示
- 新增字节级下载进度（已下载/总大小与百分比），失败可重试
- 弹窗只能通过底部按钮操作，禁用右上角关闭、点击遮罩与 ESC 关闭
- 更新清单新增 size（安装包字节大小）与 force_update（强制更新标记）字段
- 支持在 CHANGELOG 版本段落内用 `force: true` 标记强制更新

## v0.1.2

- 接入 Tauri updater：设置页可手动检查更新，新增更新对话框、下载进度与安装重启
- 新增启动时自动检查更新开关（本地偏好）
- 应用内更新说明支持从 CHANGELOG.md 读取
- GitHub Actions 在 tag 发布时生成 latest.json 并上传 Release
- 接入统一版本号更新脚本 `pnpm updateVersion`
- 补齐 Microsoft Store MSIX 双架构（x64 + arm64）打包配置与构建脚本
- 设置页外部链接改用系统浏览器打开

## v0.1.1

- 修复 MSIX 构建脚本中文注释导致的 Actions 构建编码错误
- 升级应用版本号

## v0.1.0

- 初始化仓库：Tauri 2 + Vue 3 + TypeScript + Vite 8 单包项目
- 实现基于 Win32 进程识别的专注时长统计与规则限额
- 本地 SQLite（WAL）持久化，设置 JSON 存储
- 托盘、提醒、权限与管理员启动等桌面能力
