#!/usr/bin/env bash
# 一键开发/构建脚本（bash / Git Bash / macOS / Linux）
# 自动设置 TAURI_APP_PATH，免去手动指定 src-tauri 目录的步骤。
#
# 用法（在仓库任意位置均可）：
#   ./build.sh dev      # 启动 Tauri 开发模式（完整桌面应用，含热更新）
#   ./build.sh ui       # 仅启动 Vite 前端（浏览器调试，invoke 不可用）
#   ./build.sh build    # 构建发布版（exe + NSIS 安装包）
#   ./build.sh test     # 运行 core 单元测试

set -e

# 脚本所在目录 = 仓库根目录
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export TAURI_APP_PATH="$ROOT/src-tauri"

# 首次运行自动安装前端依赖
if [ ! -d "$ROOT/ui/node_modules" ]; then
  echo "前端依赖未安装，正在安装..."
  (cd "$ROOT/ui" && npm install)
fi

case "${1:-}" in
  dev)
    cd "$ROOT/ui" && npx tauri dev
    ;;
  ui)
    cd "$ROOT/ui" && npm run dev
    ;;
  build)
    cd "$ROOT/ui" && npx tauri build
    ;;
  test)
    cd "$ROOT" && cargo test -p bookmark-core
    ;;
  *)
    echo "Bookmark Organizer 构建脚本"
    echo ""
    echo "用法: ./build.sh <命令>"
    echo "  dev    启动完整桌面应用（热更新）"
    echo "  ui     仅启动前端 Vite（浏览器调试）"
    echo "  build  构建发布版（exe + NSIS）"
    echo "  test   运行 core 单元测试"
    exit 1
    ;;
esac
