# PReferZ 发布脚本
#
# 用法（在仓库根目录执行）：
#   .\scripts\release.ps1 patch          # 发布 patch 版本（推荐）
#   .\scripts\release.ps1 minor          # 发布 minor 版本
#   .\scripts\release.ps1 major          # 发布 major 版本
#   .\scripts\release.ps1 0.2.0          # 直接指定版本号
#   .\scripts\release.ps1 patch -DryRun  # 仅预览，不实际发布
#
# 流程：
#   1. 确认工作区干净（有未提交改动则中止）
#   2. 运行 cargo fmt + clippy 检查
#   3. 校验 .agents/release-notes/v<next>.md 已写好（tag 一旦推送，CI 缺文件即失败）
#   4. cargo release 执行版本 bump → 提交 → 打 tag → push
#   5. push 触发 GitHub Actions release.yml 自动构建并创建 GitHub Release，
#      release body 取自 .agents/release-notes/v<next>.md
#
# 说明：
#   - 本地网络无法直连 crates.io，设置 CARGO_NET_OFFLINE=true 跳过版本冲突检查
#     （本项目 publish=false，不发布到 crates.io，该检查无意义）
#   - 产物由 GitHub Actions 构建，本地无需编译
#   - release note 约定见 .agents/release-notes/README.md

param(
    [Parameter(Position = 0)]
    [string]$Level = "patch",

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== PReferZ Release ===" -ForegroundColor Cyan
Write-Host "Level: $Level" -ForegroundColor Cyan

# 1. 检查工作区是否干净
$status = git status --porcelain
if ($status) {
    Write-Host "" -ForegroundColor Red
    Write-Host "ERROR: 存在未提交的更改，请先提交再发布：" -ForegroundColor Red
    git status --short
    exit 1
}

# 2. fmt + clippy 检查
Write-Host "`n--- cargo fmt --all --check ---" -ForegroundColor Yellow
cargo fmt --all --check
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: 代码未格式化，请先运行 cargo fmt" -ForegroundColor Red
    exit 1
}
Write-Host "--- cargo clippy ---" -ForegroundColor Yellow
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: clippy 存在警告" -ForegroundColor Red
    exit 1
}

# 3. 校验 release note 已写好
# release.yml 读取 .agents/release-notes/<tag>.md，缺失会让 release job 失败；
# tag-name = "v{{version}}"（见 Cargo.toml [workspace.metadata.release]）
$current = (Select-String -Path "Cargo.toml" -Pattern '^version = "([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$next = $null
if ($Level -match '^\d+\.\d+\.\d+$') {
    $next = $Level
} else {
    $p = $current -split '\.'
    switch ($Level) {
        "patch" { $next = "$([int]$p[0]).$([int]$p[1]).$([int]$p[2] + 1)" }
        "minor" { $next = "$([int]$p[0]).$([int]$p[1] + 1).0" }
        "major" { $next = "$([int]$p[0] + 1).0.0" }
    }
}

if ($next) {
    $notes = ".agents/release-notes/v$next.md"
    Write-Host "`n--- release notes: $notes ---" -ForegroundColor Yellow
    if (-not (Test-Path $notes)) {
        Write-Host "ERROR: 缺少 release note 文件 $notes" -ForegroundColor Red
        Write-Host "  从 .agents/release-notes/TEMPLATE.md 复制一份，改名为 $notes 后提交，再重新发布" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "`n--- release notes ---" -ForegroundColor Yellow
    Write-Host "WARN: 无法从 '$Level' 推断目标版本，跳过 release note 校验。" -ForegroundColor Yellow
    Write-Host "      请确认 .agents/release-notes/ 下已有与即将推送的 tag 同名的 .md 文件。" -ForegroundColor Yellow
}

# 4. cargo release
Write-Host "`n--- cargo release ---" -ForegroundColor Yellow
$env:CARGO_NET_OFFLINE = "true"
$args = @($Level, "--no-confirm")
if ($DryRun) { $args += "--dry-run" }
cargo release @args
if ($LASTEXITCODE -ne 0) {
    Write-Host "`nERROR: cargo release 失败" -ForegroundColor Red
    exit 1
}

Write-Host "`n=== 发布完成 ===" -ForegroundColor Green
if (-not $DryRun) {
    Write-Host "tag 已推送，GitHub Actions 将自动构建并创建 Release"
    Write-Host "查看进度: https://github.com/Feihei/PReferZ/actions"
}
