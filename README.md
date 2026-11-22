# 跨平台书签整理工具 Bookmark Organizer

基于 **Rust 核心 + Tauri 2** 的本地书签整理工具。导入浏览器收藏夹（Chrome / Edge / Firefox 导出的
Netscape HTML），在树形视图中整理、去重、检测链接有效性，并安全删除失效项（自动备份 + 回收站可撤销）。

> 本仓库交付第 1 期（MVP）全部 8 项功能；第 2 / 3 期功能见文末路线图。

---

## 功能（第 1 期 MVP，全部完成）

| # | 功能 | 说明 |
|---|------|------|
| 1 | 导入 / 导出 Netscape HTML | 兼容 Chrome / Edge / Firefox 导出格式；导出保留完整文件夹层级、ADD_DATE 与 ICON |
| 2 | 树形展示 + 多选批量移动 + 拖拽 | 勾选多选（含文件夹级联）、HTML5 拖拽移动（拖入文件夹或拖至条目前后）、批量移动 |
| 3 | URL 精确去重 | 标准化（scheme/host 小写、去默认端口、去空 fragment、无协议头补 http://）后 SHA-256 哈希分组；预览确认后应用 |
| 4 | 空文件夹清理 | 子树内无任何书签的文件夹（含仅含空文件夹的层级）；先预览后清理 |
| 5 | 链接有效性检测 | HEAD 优先，405/501 降级 GET（只读响应头）；超时 10s；失败重试 2 次；并发上限 8；结果标记 正常 / 可疑 / 失效 |
| 6 | HTTP 代理 | 支持配置 `http://` / `socks5://` 代理（含用户名密码），带连通性测试按钮 |
| 7 | 一键删除失效项 | 删除前 ① 自动生成带时间戳的 HTML 备份 ② 弹出确认列表 ③ 软删除进回收站，可一键恢复 |
| 8 | 检测结果持久化 | 最新结果 + 全部历史流水存入 SQLite（单项目文件，随应用数据目录） |

**判定细则**（UI 中已注明）：

- **正常**：2xx
- **可疑**：401 / 403 / 429（登录墙、风控、Cloudflare 拦截页在浏览器中往往可访问，判失效有误删风险）、其他异常状态码
- **失效**：其余 4xx / 5xx、DNS 失败、连接失败、连接 / 读取超时（10s，重试 2 次后仍失败）
- 局限声明：`HTTP 检测无法识别 Cloudflare 拦截页和 SPA 假 200，属尽力检测`

**非功能约束（已落实）**：不执行网页任何 JS（纯 HTTP 客户端）；所有请求仅 GET / HEAD，不提交任何数据；
书签数据全部本地存储，不发送任何第三方。

---

## 架构

```
bookmark-organizer/
├── crates/core/          # Rust 核心 crate（bookmark-core）：无平台 API，可复用于 Android
│   └── src/
│       ├── model.rs      #   书签树数据模型（Node / NewNode）
│       ├── urlnorm.rs    #   URL 标准化 + SHA-256 哈希（含第 2 期模糊选项预留）
│       ├── netscape.rs   #   Netscape HTML 容错解析器 / 导出器
│       ├── dedup.rs      #   精确去重（DFS 顺序 keep-first）
│       ├── empty.rs      #   空文件夹检测（两阶段：计数 + 剪枝）
│       ├── checker.rs    #   链接检测器（reqwest + rustls，Semaphore 并发）
│       ├── store.rs      #   SQLite 持久化（rusqlite bundled，软删除 / 回收站）
│       ├── backup.rs     #   时间戳 HTML 备份
│       └── tests/        #   集成测试 + wiremock mock server + 验收场景测试
├── src-tauri/            # Tauri 2 桌面壳
│   ├── src/commands.rs   #   20 个 invoke 命令（薄封装 core）
│   ├── src/state.rs      #   AppState（Store / 检测任务句柄）
│   ├── tauri.conf.json   #   单文件 exe / NSIS 配置
│   └── icons/            #   应用图标
├── ui/                   # Vue 3 + Vite + Naive UI + Pinia
│   └── src/
│       ├── components/   #   主视图 + 去重/空文件夹/失效删除/回收站/设置 五个对话框
│       ├── stores/app.ts #   Pinia 全局状态（树 / 检测进度 / 主题跟随系统）
│       └── i18n.ts       #   中英双语字典（可切换）
├── docs/sample_import.html  # 测试用示例收藏夹
└── .github/workflows/build.yml  # Windows CI：单文件 exe + NSIS 安装包
```

数据流：`ui (Vue) → invoke 命令 → commands.rs → bookmark-core → SQLite`
检测结果通过事件 `check://progress` / `check://finished` 实时推送回前端。

---

## 快速开始

### 方式一：直接下载 exe（推荐）

推送本仓库到 GitHub 后，`.github/workflows/build.yml` 会在 `windows-latest` 上自动构建：
进入仓库 **Actions → Build Windows EXE → Artifacts**，下载 `BookmarkOrganizer-windows`，包含：

- `bookmark-organizer.exe` —— 免安装单文件（WebView2 使用 Windows 10/11 系统组件）
- `BookmarkOrganizer_0.1.0_x64-setup.exe` —— NSIS 安装包（可选）

首次运行时若提示缺少 WebView2，安装包模式会自动下载安装运行时。

### 方式二：本地开发

环境要求：Rust stable（MSVC）、Node.js ≥ 20、Windows 10/11（或 Linux + webkit2gtk-4.1）。

```bash
# 1. 安装前端依赖
cd ui
npm install

# 2. 告诉 Tauri CLI 项目目录的位置（重要！）
#    Tauri CLI 只会在「当前目录及其子目录」里查找 tauri.conf.json，
#    而本项目 src-tauri/ 与 ui/ 是兄弟目录，必须用 TAURI_APP_PATH 指定：
# PowerShell：
$env:TAURI_APP_PATH = "$PWD\..\src-tauri"
# bash：
export TAURI_APP_PATH="$PWD/../src-tauri"

# 3. 开发模式（Vite 热更新）
npm run dev          # 仅前端调试（浏览器打开，invoke 不可用）
npx tauri dev        # 完整桌面应用调试

# 4. 构建发布版（产物：target/release/bookmark-organizer.exe + NSIS 安装包）
npx tauri build
```

> 若不设置 `TAURI_APP_PATH`，`npx tauri dev / build` 会报
> `Couldn't recognize the current folder as a Tauri project`——CLI 在 `ui/` 下找不到 `tauri.conf.json`（它在兄弟目录 `src-tauri/` 里）。

Linux 下编译 GUI 需要系统依赖：

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

> 注意：Windows 单文件 exe 需在 Windows（或 CI 的 windows-latest）上构建；
> Tauri 不支持从 Linux 交叉编译 MSVC 目标。

### 运行单元测试（任何平台）

```bash
cargo test -p bookmark-core
```

覆盖 41 个测试，包括：

- `netscape_tests`：Chrome / Firefox 风格夹具解析、往返一致性、残缺 HTML 容错、实体解码
- `checker_tests`（**wiremock mock server**）：200 / 404 / 5xx 重试 / 403 可疑 / HEAD→GET 兜底 /
  超时判失效 / 连接拒绝判失效 / 重定向最终 URL 记录 / 非 HTTP 协议跳过 / 取消 / **并发上限 8**（原生 TCP 服务器统计）
- `store_tests`：导入替换 / 追加、移动与非法移动校验、软删除 / 回收站 / 恢复 / 彻底删除、检测结果 upsert + 历史、设置
- `acceptance_test`：**验收标准全场景**——导入 100 条书签（3 层嵌套、10 个失效、5 个重复）→ 结构还原一致 →
  去重移除 5 项 → 检测标记 10 个失效 → 备份存在 → 导出 HTML 无失效项 → 回收站恢复全部 10 项

---

## 使用说明

1. **导入**：从 Edge / Chrome / Firefox 的「收藏夹管理器 → 导出」得到 HTML 文件，
   在工具栏「导入」中选择替换或追加模式。
2. **整理**：左侧树支持勾选多选（勾选文件夹将级联整体移动）；直接拖拽书签/文件夹到目标文件夹完成移动（拖到条目前后为排序，拖到文件夹上为移入）。
3. **右键菜单**：右键书签可打开、检测、移动或删除；右键文件夹可在该目录下直接新建子文件夹，也可检测、移动或删除。删除为软删除（进回收站，可恢复），移动目标会自动排除自身子树。输入框内仍保留系统复制/粘贴菜单。
4. **检测**：「检测全部链接」或勾选后「检测选中」；右侧面板实时显示进度与分类计数；
   每条书签显示状态标签（正常 / 可疑 / 失效 / 未检测）。
5. **去重**：「精确去重」弹出分组预览（保留项 + 重复项），确认后应用；删除项进入回收站。
6. **清理空文件夹**：先预览列表，确认后清理。
7. **删除失效链接**：弹出确认列表（含状态码与原因）；确认后自动生成
   `backups/bookmarks_backup_YYYYMMDD_HHMMSS.html` 备份并软删除。
8. **回收站**：任意删除操作均可从回收站「恢复」或「彻底删除」。
9. **设置**：界面语言（中文 / English）、主题（跟随系统 / 浅色 / 深色）、HTTP 代理与连通性测试、
   数据库文件位置。

数据文件：`%APPDATA%/com.bookmarkorganizer.desktop/bookmarks.db`（Windows）。

---

## 验收标准核对表

| 规格要求 | 实现 | 自动化验证 |
|---|---|---|
| 导入含 100 条书签（嵌套 3 层、10 失效、5 重复）的 HTML，树形结构还原一致 | `acceptance_test` 导入断言 | ✅ |
| 检测完成后 10 个失效项被标记 | wiremock 路由（404 / 连接拒绝 / 超时）+ stats 断言 | ✅ |
| 一键删除后导出 HTML 中失效项已移除 | 导出内容断言 | ✅ |
| 备份文件存在（含时间戳） | `backup::create_backup` 文件名断言 | ✅ |
| 回收站可恢复全部 10 项 | 逐条恢复 + 书签数回到 95 断言 | ✅ |
| 全部功能提供单元测试（网络模块使用 mock server） | wiremock + 原生 TCP 服务器 | ✅（41 个测试） |
| HEAD 优先、GET 兜底；4xx/5xx、DNS、超时(10s) 判失效；重试 2 次；并发 8 | `checker.rs` + `checker_tests` | ✅ |
| 正常 / 可疑 / 失效三态（401/403/429 → 可疑防误删） | `classify_status` + 测试 | ✅ |
| 代理配置 + 测试 | `set_proxy` / `test_proxy` 命令 | UI 手动验证 |
| 删除前自动备份 + 确认列表 + 软删除撤销 | `delete_dead` 流程 + UI 三步确认 | ✅ / 手动 |

## 路线图

- **第 2 期**：模糊去重（`urlnorm::FuzzyOptions` 已预留 ignore_query / ignore_case / ignore_trailing_slash /
  ignore_fragment 四个开关）、合并重复文件夹、增量检测（结果缓存已有 `check_results` 表支撑）、
  重定向链接自动更新为最终地址（检测器已记录 `final_url`）、域名 + 关键词规则自动归类（应用前预览）、WebDAV 备份同步
- **第 3 期（可选）**：AI 自动打标签 / 摘要（OpenAI 兼容 API 或本地 Ollama，仅在用户显式配置后启用、发送前确认）、
  Tauri 2 Android 打包（core crate 无平台 API，可直接复用）

## 已知限制

- HTTP 检测无法识别 Cloudflare 拦截页与 SPA 假 200（尽力检测，UI 已注明）
- `javascript:` / `mailto:` / `chrome://` 等非 HTTP 协议书签不做检测，状态保持「未检测」，也不会被一键删除误伤
- 移动端（Android）打包路径已预留，本期不实现
