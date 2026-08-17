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
#   3. cargo release 执行版本 bump → 提交 → 打 tag → push
#   4. push 触发 GitHub Actions release.yml 自动构建并创建 GitHub Release
#
# 说明：
#   - 本地网络无法直连 crates.io，设置 CARGO_NET_OFFLINE=true 跳过版本冲突检查
#     （本项目 publish=false，不发布到 crates.io，该检查无意义）
#   - 产物由 GitHub Actions 构建，本地无需编译

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

# 3. cargo release
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
