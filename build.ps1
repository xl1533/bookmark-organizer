# 一键开发/构建脚本（Windows PowerShell）
# 自动设置 TAURI_APP_PATH，免去手动指定 src-tauri 目录的步骤。
#
# 用法（在仓库任意位置均可）：
#   .\build.ps1 dev      # 启动 Tauri 开发模式（完整桌面应用，含热更新）
#   .\build.ps1 ui       # 仅启动 Vite 前端（浏览器调试，invoke 不可用）
#   .\build.ps1 build    # 构建发布版（exe + NSIS 安装包）
#   .\build.ps1 test     # 运行 core 单元测试

$ErrorActionPreference = "Stop"

# 脚本所在目录 = 仓库根目录
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$env:TAURI_APP_PATH = Join-Path $Root "src-tauri"

# 首次运行自动安装前端依赖
$NodeModules = Join-Path $Root "ui\node_modules"
if (-not (Test-Path $NodeModules)) {
    Write-Host "前端依赖未安装，正在安装..."
    Push-Location (Join-Path $Root "ui")
    try { npm install } finally { Pop-Location }
}

$Cmd = if ($args.Count -gt 0) { $args[0] } else { "" }
Push-Location (Join-Path $Root "ui")
try {
    switch ($Cmd) {
        "dev"   { npx tauri dev }
        "ui"    { npm run dev }
        "build" { npx tauri build }
        "test"  { Pop-Location; Push-Location $Root; cargo test -p bookmark-core }
        default {
            Write-Host "Bookmark Organizer 构建脚本"
            Write-Host ""
            Write-Host "用法: .\build.ps1 <命令>"
            Write-Host "  dev    启动完整桌面应用（热更新）"
            Write-Host "  ui     仅启动前端 Vite（浏览器调试）"
            Write-Host "  build  构建发布版（exe + NSIS）"
            Write-Host "  test   运行 core 单元测试"
        }
    }
} finally {
    Pop-Location
}
