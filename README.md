# 跨平台书签整理工具 Bookmark Organizer

基于 **Rust 核心 + Tauri 2** 的本地书签整理工具。导入浏览器收藏夹（Chrome / Edge / Firefox 导出的
Netscape HTML），在树形视图中整理、去重、检测链接有效性，并安全删除失效项（自动备份 + 回收站可撤销）。

> 第 1 期（MVP）8 项、第 2 期 6 项功能均已完成；后续方向见 [ROADMAP.md](ROADMAP.md)。本工具**完全本地运行**，不执行网页 JS、不提交任何数据、不上传书签（WebDAV 除外，且完全由你主动触发）。

---

## 功能一览

### 第 1 期（MVP）

| # | 功能 | 说明 |
|---|------|------|
| 1 | 导入 / 导出 Netscape HTML | 兼容 Chrome / Edge / Firefox 导出格式；导出保留完整文件夹层级、ADD_DATE 与 ICON |
| 2 | 树形展示 + 多选批量移动 + 拖拽 | 勾选多选（文件夹级联）；拖拽书签/文件夹到目标文件夹，拖到条目前后为排序 |
| 3 | URL 精确去重 | 标准化（scheme/host 小写、去默认端口、去空 fragment、无协议头补 http://）后 SHA-256 哈希分组；预览确认后应用 |
| 4 | 空文件夹清理 | 子树内无任何书签的文件夹（含仅含空文件夹的层级）；先预览后清理 |
| 5 | 链接有效性检测 | HEAD 优先、405/501 降级 GET（只读响应头）；超时 10s；失败重试 2 次；全局并发 8 + 同域并发 2；取消即时响应；结果标记 正常 / 可疑 / 失效 |
| 6 | HTTP 代理 | 支持 `http://` / `socks5://` 代理（含用户名密码），带连通性测试 |
| 7 | 一键删除失效项 | 删除前 ① 自动生成带时间戳的 HTML 备份 ② 弹出确认列表 ③ 软删除进回收站，可一键恢复 |
| 8 | 检测结果持久化 | 最新结果 + 全部历史流水存入 SQLite（单项目文件，随应用数据目录） |

**判定细则**（UI 中已注明）：

- **正常**：2xx
- **可疑**：401 / 403 / 429（登录墙、风控、Cloudflare 拦截页在浏览器中往往可访问，判失效有误删风险）、其他异常状态码
- **失效**：其余 4xx / 5xx、DNS 失败、连接失败、连接 / 读取超时（10s，重试 2 次后仍失败）
- 局限声明：`HTTP 检测无法识别 Cloudflare 拦截页和 SPA 假 200，属尽力检测`

### 第 2 期

| # | 功能 | 说明 |
|---|------|------|
| 9 | 模糊去重 | 可配置「忽略 query / 路径大小写 / 末尾斜杠 / fragment」四个开关，开关持久化；切换即重算预览；应用后重复项软删除进回收站；全关时等同精确去重 |
| 10 | 合并重复文件夹 | 同名文件夹（忽略大小写与首尾空白）分组预览，保留项可改；子项与同名子文件夹全部递归合并，顺序不变；应用前自动 HTML 备份 |
| 11 | 增量检测 | 只检测新增、URL 严格哈希变更、超过 N 天（默认 7）未检测的项；跳过项保留原检测状态；复用进度/完成事件与取消机制 |
| 12 | 更新重定向 | 将检测到的最终地址预览后写回书签，并同步刷新检测缓存的 URL/哈希；写回前自动 HTML 备份 |
| 13 | 规则归类 | 域名 / URL 关键词 / 标题关键词的包含匹配规则（可增删改排序、持久化）；先预览命中再逐条勾选应用（默认不选），自动建目录、冲突不覆盖 |
| 14 | WebDAV 备份同步 | 设置页手动配置服务器（默认仅 https）、Basic 认证、代理复用；上传自动备份到自配服务器，远端时间戳文件名永不覆盖；下载列表选文件后按替换/追加确认导入 |

### 交互与界面

- **右键菜单**：右键书签可打开、检测、移动或删除；右键文件夹可在该目录下直接新建子文件夹，也可检测、移动或删除。删除为软删除（进回收站可恢复），移动目标自动排除自身子树。输入框内保留系统复制/粘贴菜单。
- **界面**：中英双语、主题跟随系统（浅/深可切换）；工具栏按「导入导出 / 检测 / 整理工具」分组，整理类操作先预览后应用；检测面板卡片化展示分类计数。
- **大规模书签**：书签树开启虚拟滚动，几千上万条节点下滚动依然流畅；检测采用全局 + 同域两级限流。
- **体积**：发布构建使用体积优先的编译配置（LTO、`opt-level=z`、`panic=abort`、strip），Windows 产物压缩包约 **5.3 MB**（免安装 exe + NSIS 安装包）。

### 隐私边界：链接检测 ≠ WebDAV 上传

- **链接检测**仅向目标网址发送 HEAD（不支持时降级 GET）请求读取响应头，**不提交任何数据、不执行 JS、不外发本地书签**。
- **WebDAV** 完全由你主动开启：你在设置页填写自己的服务器 URL 与凭据，点击「上传备份」时，才会把**完整书签 HTML 备份** PUT 到你自己的私有服务器；不存在后台自动同步，也不做自动双向合并或覆盖裁决。
- 两者是**不同的功能与不同的数据流向**：即使你从未配置 WebDAV，检测也照常工作、且不会上报任何书签数据。
- 默认只允许 https（仅 localhost / 127.0.0.1 可在明确提示后开启 http）；凭据以明文保存在本机 SQLite `settings` 表，**不会自动上传**，请自行控制数据库文件访问权限。
- 备份内容**不加密**，传输依靠 HTTPS；服务器管理员可以读取备份内容。这是你自有服务器的可信边界，请在配置前知悉。
- 上传使用带时间戳的文件名并附带 `If-None-Match: *`，**永不覆盖**已有远端文件；下载导入前必须选择「替换 / 追加」并确认，临时下载文件导入后立即删除（无论成功失败）。

---

## 架构

```
bookmark-organizer/
├── crates/core/            # Rust 核心 crate（bookmark-core）：无平台 API，可复用于 Android
│   └── src/
│       ├── model.rs        #   书签树数据模型（Node / NewNode）
│       ├── urlnorm.rs      #   URL 标准化 + SHA-256 哈希（严格 hash 与模糊选项隔离）
│       ├── netscape.rs     #   Netscape HTML 容错解析器 / 导出器
│       ├── dedup.rs        #   精确 / 模糊去重（DFS 顺序 keep-first）
│       ├── empty.rs        #   空文件夹检测（两阶段：计数 + 剪枝）
│       ├── checker.rs      #   链接检测器（reqwest + rustls，Semaphore 并发）
│       ├── rules.rs        #   第 2 期：规则模型与匹配引擎
│       ├── webdav.rs       #   第 2 期：WebDAV 客户端（PROPFIND / PUT / GET）
│       ├── store.rs        #   SQLite 持久化（rusqlite bundled，软删除 / 回收站）
│       ├── backup.rs       #   时间戳 HTML 备份
│       └── tests/          #   集成测试 + wiremock mock server + 验收场景测试
├── src-tauri/              # Tauri 2 桌面壳
│   ├── src/commands.rs     #   invoke 命令（薄封装 core，含第 2 期各功能）
│   ├── src/state.rs        #   AppState（Store / 检测任务句柄）
│   ├── tauri.conf.json     #   单文件 exe / NSIS 配置
│   └── icons/              #   应用图标
├── ui/                     # Vue 3 + Vite + Naive UI + Pinia
│   └── src/
│       ├── components/     #   主视图 + 工具栏/检测面板/状态栏 + 各功能对话框
│       ├── composables/tree.ts  # 公共树工具（抽自 MainView）
│       ├── stores/app.ts   #   Pinia 全局状态（树 / 检测进度 / 主题）
│       └── i18n.ts         #   中英双语字典（键集由类型强制对齐）
├── docs/sample_import.html # 测试用示例收藏夹
└── .github/workflows/build.yml  # Windows CI：单文件 exe + NSIS 安装包
```

数据流：`ui (Vue) → invoke 命令 → commands.rs → bookmark-core → SQLite`。
检测结果通过事件 `check://progress` / `check://finished` 实时推送回前端。

---

## 快速开始

### 方式一：下载 CI 构建产物（推荐）

每次推送到 `main` 后，`.github/workflows/build.yml` 会在 `windows-latest` 上自动构建：
进入仓库 **Actions → Build Windows EXE → Artifacts**，下载 `BookmarkOrganizer-windows`，包含：

- `bookmark-organizer.exe` —— 免安装单文件（WebView2 使用 Windows 10/11 系统组件）
- `BookmarkOrganizer_1.0.0_x64-setup.exe` —— NSIS 安装包（可选）

首次运行时若提示缺少 WebView2，安装包模式会自动下载安装运行时。

> 仅修改文档（README / ROADMAP / docs）不会触发构建——工作流已配置 `paths-ignore` 过滤。

### 方式二：本地开发

环境要求：Rust stable（MSVC）、Node.js ≥ 20、Windows 10/11（或 Linux + webkit2gtk-4.1）。

**用封装脚本（推荐）**——脚本会自动设置 `TAURI_APP_PATH` 并在首次运行时安装前端依赖，无需手动配置：

```bash
# bash / Git Bash / macOS / Linux
./build.sh dev       # 完整桌面应用（热更新）
./build.sh ui        # 仅前端 Vite（浏览器调试）
./build.sh build     # 构建发布版（exe + NSIS）
./build.sh test      # core 单元测试
```

```powershell
# Windows PowerShell
.\build.ps1 dev
.\build.ps1 build
```

> 为什么需要脚本：Tauri CLI 只在「当前目录及子目录」查找 `tauri.conf.json`，而本项目
> `src-tauri/` 与 `ui/` 是兄弟目录，直接 `npx tauri dev/build` 会报
> `Couldn't recognize the current folder as a Tauri project`。脚本帮你设好了 `TAURI_APP_PATH`。

**手动方式**（等价于脚本做的事）：

```bash
cd ui && npm install
export TAURI_APP_PATH="$PWD/../src-tauri"   # PowerShell: $env:TAURI_APP_PATH = "$PWD\..\src-tauri"
npm run dev        # 仅前端
npx tauri dev      # 完整应用
npx tauri build    # 发布版
```

Linux 下编译 GUI 需要系统依赖：

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

> Windows 单文件 exe 需在 Windows（或 CI 的 windows-latest）上构建；Tauri 不支持从 Linux 交叉编译 MSVC 目标。

### 运行单元测试

```bash
cargo test -p bookmark-core
```

共 57 个测试，覆盖核心算法与各功能的单元测试和网络 mock，包括：

- `netscape_tests`：Chrome / Firefox 风格夹具解析、往返一致性、残缺 HTML 容错、实体解码
- `checker_tests`（**wiremock mock server**）：200 / 404 / 5xx 重试 / 403 可疑 / HEAD→GET 兜底 /
  超时判失效 / 连接拒绝判失效 / 重定向最终 URL 记录 / 非 HTTP 协议跳过 / 取消 / **并发上限 8**
- `store_tests`：导入替换 / 追加、移动与非法移动校验、软删除 / 回收站 / 恢复 / 彻底删除、检测结果 upsert + 历史、设置、合并文件夹顺序与递归
- `rules.rs` / `webdav.rs` / `urlnorm.rs`：规则匹配与首个命中、WebDAV URL 安全校验与 PROPFIND 解析、模糊选项与严格 hash 隔离
- `acceptance_test`：**验收标准全场景**——导入 100 条书签（3 层嵌套、10 个失效、5 个重复）→ 结构还原一致 →
  去重移除 5 项 → 检测标记 10 个失效 → 备份存在 → 导出 HTML 无失效项 → 回收站恢复全部 10 项

---

## 使用说明

1. **导入**：从 Edge / Chrome / Firefox 的「收藏夹管理器 → 导出」得到 HTML 文件，在工具栏「导入」中选择替换或追加模式。
2. **整理**：左侧树支持勾选多选（勾选文件夹将级联整体移动）；拖拽书签/文件夹到目标文件夹完成移动（拖到条目前后为排序，拖到文件夹上为移入）。
3. **右键菜单**：右键书签可打开、检测、移动或删除；右键文件夹可在该目录下直接新建子文件夹，也可检测、移动或删除。删除为软删除（进回收站可恢复），移动目标会自动排除自身子树。输入框内仍保留系统复制/粘贴菜单。
4. **检测**：「检测全部链接」或勾选后「检测选中」；「增量检测」只查新增 / 变更 / 超期的项；右侧面板实时显示进度与分类计数，每条书签显示状态标签。
5. **去重**：「精确去重」与「模糊去重」（可配置四个放宽开关）均弹出分组预览（保留项 + 重复项），确认后应用；删除项进入回收站。
6. **清理空文件夹**：先预览列表，确认后清理。
7. **删除失效链接**：弹出确认列表（含状态码与原因）；确认后自动生成 `backups/bookmarks_backup_YYYYMMDD_HHMMSS.html` 备份并软删除。
8. **回收站**：任意删除操作均可从回收站「恢复」或「彻底删除」。
9. **设置**：界面语言（中文 / English）、主题（跟随系统 / 浅色 / 深色）、HTTP 代理与连通性测试、数据库文件位置、增量检测超期天数。
10. **整理工具**：工具栏下拉菜单提供精确/模糊去重、合并重复文件夹、更新重定向、规则归类；每个操作均先预览确认，破坏性操作前自动生成 HTML 备份（撤销 = 用该备份以替换模式重新导入）。
11. **WebDAV 备份**：在设置页展开「WebDAV 备份」，填写服务器 URL / 用户名 / 密码 / 远端目录，可先「测试连接」再「保存配置」；「上传备份」生成时间戳 HTML 并上传（永不覆盖）；「刷新文件列表」后选择远端文件、点「下载并导入」，在确认框中选择替换或追加。详见上方隐私边界说明。

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
| 全部功能提供单元测试（网络模块使用 mock server） | wiremock + 原生 TCP 服务器 | ✅（57 个测试） |
| HEAD 优先、GET 兜底；4xx/5xx、DNS、超时(10s) 判失效；重试 2 次；全局并发 8 + 同域 2 | `checker.rs` + `checker_tests` | ✅ |
| 正常 / 可疑 / 失效三态（401/403/429 → 可疑防误删） | `classify_status` + 测试 | ✅ |
| 代理配置 + 测试 | `set_proxy` / `test_proxy` 命令 | UI 手动验证 |
| 删除前自动备份 + 确认列表 + 软删除撤销 | `delete_dead` 流程 + UI 三步确认 | ✅ / 手动 |

---

## 设计取舍与不做的事

本工具定位为一个**小而专的本地整理工具**，刻意保持简单：

- **不接入任何智能服务**：不做自动打标签、自动摘要、大模型归类——没有相关入口、配置或文案，也不会向任何外部智能服务发送数据。
- **明确排除**：云同步服务端、浏览器扩展联动、网页截图、执行 JS 验证——均不在计划内。WebDAV 只是把备份文件送到你自配的私有服务器，不构成云同步服务端。
- **凭据存储**：WebDAV 密码以明文保存在本机 SQLite 的 `settings` 表（与代理密码一致），未接入系统钥匙串。请自行控制数据库文件访问权限；传输全程 HTTPS。
- **性能取舍**：发布构建采用体积优先（`opt-level=z`），本工具的瓶颈在网络检测而非 CPU 计算；检测采用全局并发 8 + 同域并发 2 的限流，避免对少数站点造成压力。

## 已知限制

- HTTP 检测无法识别 Cloudflare 拦截页与 SPA 假 200（尽力检测，UI 已注明）。
- `javascript:` / `mailto:` / `chrome://` 等非 HTTP 协议书签不做检测，状态保持「未检测」，也不会被一键删除误伤。
- WebDAV PROPFIND 响应使用轻量解析，兼容主流服务器，遇到极不规范 XML 可能需后续引入专门解析库。
- 移动端（Android）打包路径已预留，暂不实现。
