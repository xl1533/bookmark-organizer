import { ref } from "vue";

export type Locale = "zh" | "en";

const zh = {
  appName: "书签整理工具",
  // 工具栏
  importMenu: "导入",
  importReplace: "导入（替换现有书签）",
  importAppend: "导入（追加到根目录）",
  exportBtn: "导出",
  checkAll: "检测全部链接",
  checkSelected: "检测选中",
  stopCheck: "停止检测",
  dedup: "精确去重",
  emptyFolders: "清理空文件夹",
  deleteDead: "删除失效链接",
  trash: "回收站",
  settings: "设置",
  // 树
  searchPlaceholder: "搜索书签",
  checkedCount: "已选 {n} 项",
  noBookmarks: "暂无书签，请先导入",
  loading: "加载中…",
  // 状态
  statusOk: "正常",
  statusSuspicious: "可疑",
  statusDead: "失效",
  statusUnchecked: "未检测",
  // 统计栏
  statBookmarks: "书签",
  statFolders: "文件夹",
  statOk: "正常",
  statSuspicious: "可疑",
  statDead: "失效",
  statUnchecked: "未检测",
  statTrash: "回收站",
  dbPath: "数据库",
  // 检测提示
  checkDisclaimer:
    "HTTP 检测无法识别 Cloudflare 拦截页和 SPA 假 200，属尽力检测。检测仅使用 GET/HEAD，不执行网页 JS，不提交任何数据。",
  checkProgressLabel: "检测进度",
  confirmBody: "确认",
  cancel: "取消",
  confirm: "确认",
  close: "关闭",
  apply: "应用",
  restore: "恢复",
  purge: "彻底删除",
  refresh: "刷新",
  // 导入/导出
  importedOk: "导入成功：{b} 个书签，{f} 个文件夹",
  exportedOk: "导出成功（{n} 项）",
  selectHtmlFile: "请选择收藏夹 HTML 文件",
  htmlFilterName: "收藏夹 HTML",
  // 去重
  dedupTitle: "精确去重（标准化后哈希）",
  dedupColUrl: "标准化 URL",
  dedupColKeep: "保留项",
  dedupColDups: "重复项",
  dedupEmpty: "未发现重复项",
  dedupApplyOk: "已删除 {n} 个重复项（可在回收站撤销）",
  dedupHint: "每组保留树形顺序中的第一项，其余项将被软删除并进入回收站。",
  // 空文件夹
  emptyTitle: "待清理的空文件夹",
  emptyColFolder: "文件夹",
  emptyColPath: "路径",
  emptyHint:
    "「空文件夹」指整棵子树内没有任何书签的文件夹。删除后可从回收站恢复。",
  emptyApplyOk: "已清理 {n} 个空文件夹（可在回收站撤销）",
  emptyNone: "没有空文件夹",
  // 失效删除
  deadTitle: "确认删除失效链接",
  deadColTitle: "标题",
  deadColUrl: "URL",
  deadColCode: "状态码",
  deadColReason: "原因",
  deadColPath: "所在路径",
  deadBackupNote: "删除前将自动生成带时间戳的 HTML 备份；删除后可从回收站撤销（软删除）。",
  deadDeleteBtn: "备份并删除",
  deadDeleted: "已删除 {n} 个失效项",
  deadBackupPath: "备份文件",
  deadEmpty: "没有已标记的失效链接，请先执行检测。",
  // 回收站
  trashTitle: "回收站",
  trashColTitle: "名称",
  trashColKind: "类型",
  trashColCount: "含书签",
  trashColTime: "删除时间",
  kindFolder: "文件夹",
  kindBookmark: "书签",
  trashEmpty: "回收站为空",
  trashRestored: "已恢复",
  trashPurged: "已彻底删除",
  trashRestoreAll: "恢复全部",
  // 设置
  settingsTitle: "设置",
  settingsLanguage: "界面语言",
  settingsTheme: "界面主题",
  themeAuto: "跟随系统",
  themeLight: "浅色",
  themeDark: "深色",
  settingsProxy: "HTTP 代理",
  proxyPlaceholder: "如 http://127.0.0.1:7890 或 socks5://...（留空为直连）",
  proxyTest: "测试连接",
  proxySave: "保存",
  proxySaved: "代理设置已保存",
  proxyTesting: "测试中…",
  settingsDb: "数据库位置",
  // 检测
  checkStarted: "开始检测 {n} 个链接",
  checkFinished: "检测完成：正常 {ok} · 可疑 {susp} · 失效 {dead}",
  checkCanceled: "检测已停止",
  checkNothing: "没有可检测的链接",
  selectSomeFirst: "请先勾选要操作的书签",
  // 移动
  movedOk: "已移动 {n} 项",
  moveErrorOwnSubtree: "不能把节点移动到其自身子树内",
  // 错误
  opFailed: "操作失败",
};

export type DictKey = keyof typeof zh;

const en: Record<DictKey, string> = {
  appName: "Bookmark Organizer",
  importMenu: "Import",
  importReplace: "Import (replace all bookmarks)",
  importAppend: "Import (append to root)",
  exportBtn: "Export",
  checkAll: "Check all links",
  checkSelected: "Check selected",
  stopCheck: "Stop checking",
  dedup: "Exact dedup",
  emptyFolders: "Clean empty folders",
  deleteDead: "Delete dead links",
  trash: "Trash",
  settings: "Settings",
  searchPlaceholder: "Search bookmarks",
  checkedCount: "{n} selected",
  noBookmarks: "No bookmarks yet. Import a bookmarks HTML file first.",
  loading: "Loading…",
  statusOk: "OK",
  statusSuspicious: "Suspicious",
  statusDead: "Dead",
  statusUnchecked: "Unchecked",
  statBookmarks: "Bookmarks",
  statFolders: "Folders",
  statOk: "OK",
  statSuspicious: "Suspicious",
  statDead: "Dead",
  statUnchecked: "Unchecked",
  statTrash: "Trash",
  dbPath: "Database",
  checkDisclaimer:
    "HTTP checking cannot detect Cloudflare challenge pages or SPA fake-200 responses. Best-effort only. Requests use GET/HEAD only; no JS is executed and no data is submitted.",
  checkProgressLabel: "Check progress",
  confirmBody: "Confirm",
  cancel: "Cancel",
  confirm: "OK",
  close: "Close",
  apply: "Apply",
  restore: "Restore",
  purge: "Delete forever",
  refresh: "Refresh",
  importedOk: "Imported: {b} bookmarks, {f} folders",
  exportedOk: "Exported ({n} items)",
  selectHtmlFile: "Select a bookmarks HTML file",
  htmlFilterName: "Bookmarks HTML",
  dedupTitle: "Exact dedup (normalized URL hash)",
  dedupColUrl: "Normalized URL",
  dedupColKeep: "Keep",
  dedupColDups: "Duplicates",
  dedupEmpty: "No duplicates found",
  dedupApplyOk: "Removed {n} duplicates (undoable in Trash)",
  dedupHint: "The first item in tree order is kept; the rest are soft-deleted and moved to Trash.",
  emptyTitle: "Empty folders to clean",
  emptyColFolder: "Folder",
  emptyColPath: "Path",
  emptyHint:
    "An empty folder contains no bookmarks anywhere in its subtree. Deleted folders can be restored from Trash.",
  emptyApplyOk: "Cleaned {n} empty folders (undoable in Trash)",
  emptyNone: "No empty folders",
  deadTitle: "Confirm deleting dead links",
  deadColTitle: "Title",
  deadColUrl: "URL",
  deadColCode: "Code",
  deadColReason: "Reason",
  deadColPath: "Location",
  deadBackupNote:
    "A timestamped HTML backup is created automatically before deletion; deleted items can be restored from Trash (soft delete).",
  deadDeleteBtn: "Backup & delete",
  deadDeleted: "Deleted {n} dead links",
  deadBackupPath: "Backup file",
  deadEmpty: "No dead links marked yet. Run a link check first.",
  trashTitle: "Trash",
  trashColTitle: "Name",
  trashColKind: "Type",
  trashColCount: "Bookmarks",
  trashColTime: "Deleted at",
  kindFolder: "Folder",
  kindBookmark: "Bookmark",
  trashEmpty: "Trash is empty",
  trashRestored: "Restored",
  trashPurged: "Deleted forever",
  trashRestoreAll: "Restore all",
  settingsTitle: "Settings",
  settingsLanguage: "Language",
  settingsTheme: "Theme",
  themeAuto: "Follow system",
  themeLight: "Light",
  themeDark: "Dark",
  settingsProxy: "HTTP proxy",
  proxyPlaceholder: "e.g. http://127.0.0.1:7890 or socks5://… (empty = direct)",
  proxyTest: "Test",
  proxySave: "Save",
  proxySaved: "Proxy settings saved",
  proxyTesting: "Testing…",
  settingsDb: "Database location",
  checkStarted: "Checking {n} links",
  checkFinished: "Finished: OK {ok} · Suspicious {susp} · Dead {dead}",
  checkCanceled: "Check stopped",
  checkNothing: "No links to check",
  selectSomeFirst: "Select bookmarks first",
  movedOk: "Moved {n} items",
  moveErrorOwnSubtree: "Cannot move a node into its own subtree",
  opFailed: "Operation failed",
};

const dicts: Record<Locale, Record<DictKey, string>> = { zh, en };

export const locale = ref<Locale>(
  (localStorage.getItem("bo.locale") as Locale) ??
    (navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en")
);

export function setLocale(l: Locale) {
  locale.value = l;
  localStorage.setItem("bo.locale", l);
}

export function useI18n() {
  const t = (key: DictKey, params?: Record<string, string | number>): string => {
    let s = dicts[locale.value][key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        s = s.replace(`{${k}}`, String(v));
      }
    }
    return s;
  };
  return { t, locale, setLocale };
}
