# v0.0.0

> 一句话概括这个版本。写在最前面，Release 列表和通知里只显示这一行。

## 亮点

- **<特性名>**：<做了什么>（`crates/...` 或 spec 链接）
- **<特性名>**：<做了什么>

## 修复

- Fix <问题>（`abcdef1`）
- Fix <问题>

## 内部

- chore: <重构 / 依赖升级 / CI 调整>

## 破坏性变更

- 无

## macOS 安装（首次运行需放行）

1. 下载 `preferz-<版本>-macos-universal.tar.gz`，双击解压得到 **PReferZ.app**（Intel 与 Apple Silicon 通用，无需选版本）
2. 双击 PReferZ.app，系统提示"无法验证开发者"→ 点「完成」
3. 打开 **系统设置 → 隐私与安全性**，滚动到最底部，点「仍要打开」，输入密码或 Touch ID 确认
4. 之后直接双击即可启动；升级新版本后需重复第 2~3 步

**English**

1. Download `preferz-<version>-macos-universal.tar.gz` and double-click to extract **PReferZ.app** (universal build, Intel & Apple Silicon)
2. Double-click PReferZ.app — macOS says "cannot verify the developer" → click "Done"
3. Open **System Settings → Privacy & Security**, scroll to the bottom, click **"Open Anyway"**, confirm with password / Touch ID
4. Subsequent launches open normally; repeat steps 2–3 after each upgrade

> 注意：macOS 15 (Sequoia) 起，右键"打开"不再能放行未公证应用，必须走系统设置里的「仍要打开」。本应用为个人开源项目，未购买开发者证书公证，此步骤无法省略。
> Note: since macOS 15 (Sequoia), right-click → "Open" no longer bypasses Gatekeeper for unnotarized apps — use "Open Anyway" in System Settings. This app is not notarized (no paid developer certificate), so this step cannot be skipped.
