import { invoke } from "@tauri-apps/api/core";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import type {
  CheckFinishedEvent,
  ClassifyChoiceDto,
  ClassifyItemDto,
  DeadItem,
  DeleteDeadResult,
  DupGroup,
  FuzzyOptions,
  ImportSummary,
  IncrementalPlanDto,
  ItemDto,
  MergeChoiceDto,
  MergeGroupDto,
  MergeResultDto,
  ProxyTestResult,
  RedirectApplyDto,
  RedirectDto,
  Rule,
  RuleInput,
  Stats,
  TrashEntry,
  TreeNode,
  WebDavConfig,
  WebDavFileDto,
  WebDavUploadDto,
} from "./types";

export const api = {
  getTree: (): Promise<TreeNode[]> => invoke("get_tree"),
  getStats: (): Promise<Stats> => invoke("get_stats"),
  getDbPath: (): Promise<string> => invoke("get_db_path"),

  importHtml: (path: string, mode: "replace" | "append"): Promise<ImportSummary> =>
    invoke("import_html", { path, mode }),
  exportHtml: (path: string): Promise<number> => invoke("export_html", { path }),

  moveNodes: (ids: number[], targetId: number, index: number): Promise<void> =>
    invoke("move_nodes", { ids, targetId, index }),
  createFolder: (parentId: number, title: string, index: number): Promise<number> =>
    invoke("create_folder", { parentId, title, index }),
  deleteNodes: (ids: number[]): Promise<number> => invoke("delete_nodes", { ids }),

  // 用系统默认浏览器打开链接
  openUrl: (url: string): Promise<void> => tauriOpenUrl(url),

  findEmptyFolders: (): Promise<ItemDto[]> => invoke("find_empty_folders"),
  deleteEmptyFolders: (ids: number[]): Promise<number> =>
    invoke("delete_empty_folders", { ids }),

  dedupPreview: (): Promise<DupGroup[]> => invoke("dedup_preview"),
  dedupApply: (): Promise<number> => invoke("dedup_apply"),

  startCheck: (scope: "all" | "selected", ids: number[]): Promise<{ total: number }> =>
    invoke("start_check", { scope, ids }),
  cancelCheck: (): Promise<void> => invoke("cancel_check"),

  listDead: (): Promise<DeadItem[]> => invoke("list_dead"),
  deleteDead: (): Promise<DeleteDeadResult> => invoke("delete_dead"),

  trashList: (): Promise<TrashEntry[]> => invoke("trash_list"),
  trashRestore: (entryId: number): Promise<void> => invoke("trash_restore", { entryId }),
  trashPurge: (entryId: number): Promise<void> => invoke("trash_purge", { entryId }),

  getProxy: (): Promise<string> => invoke("get_proxy"),
  setProxy: (proxy: string): Promise<void> => invoke("set_proxy", { proxy }),
  testProxy: (proxy: string): Promise<ProxyTestResult> => invoke("test_proxy", { proxy }),

  // 通用设置（本机 SQLite settings 表）
  getSetting: (key: string): Promise<string | null> => invoke("get_setting", { key }),
  setSetting: (key: string, value: string): Promise<void> =>
    invoke("set_setting", { key, value }),

  // ---------------- 第 2 期命令封装（后端命令由 T02-T05 实现并注册） ----------------
  // #9 模糊去重：preview 与 apply 必须使用同一份 opts
  fuzzyDedupPreview: (opts: FuzzyOptions): Promise<DupGroup[]> =>
    invoke("fuzzy_dedup_preview", { opts }),
  fuzzyDedupApply: (opts: FuzzyOptions): Promise<number> =>
    invoke("fuzzy_dedup_apply", { opts }),

  // #11 增量检测
  incrementalPreview: (days: number): Promise<IncrementalPlanDto> =>
    invoke("incremental_preview", { days }),
  startIncrementalCheck: (days: number): Promise<{ total: number }> =>
    invoke("start_incremental_check", { days }),

  // #12 重定向更新
  listRedirects: (): Promise<RedirectDto[]> => invoke("list_redirects"),
  applyRedirects: (ids: number[]): Promise<RedirectApplyDto> =>
    invoke("apply_redirects", { ids }),

  // #10 合并文件夹
  mergeFoldersPreview: (): Promise<MergeGroupDto[]> => invoke("merge_folders_preview"),
  mergeFoldersApply: (groups: MergeChoiceDto[]): Promise<MergeResultDto> =>
    invoke("merge_folders_apply", { groups }),

  // #13 规则归类
  ruleList: (): Promise<Rule[]> => invoke("rule_list"),
  ruleAdd: (rule: RuleInput): Promise<Rule> => invoke("rule_add", { rule }),
  ruleUpdate: (rule: Rule): Promise<void> => invoke("rule_update", { rule }),
  ruleDelete: (id: string): Promise<void> => invoke("rule_delete", { id }),
  ruleReorder: (ids: string[]): Promise<void> => invoke("rule_reorder", { ids }),
  classifyPreview: (
    includeClassified: boolean,
    selectedIds: number[]
  ): Promise<ClassifyItemDto[]> =>
    invoke("classify_preview", { includeClassified, selectedIds }),
  classifyApply: (items: ClassifyChoiceDto[]): Promise<number> =>
    invoke("classify_apply", { items }),

  // #14 WebDAV 备份同步
  webdavTest: (cfg: WebDavConfig): Promise<ProxyTestResult> => invoke("webdav_test", { cfg }),
  webdavUpload: (): Promise<WebDavUploadDto> => invoke("webdav_upload"),
  webdavList: (): Promise<WebDavFileDto[]> => invoke("webdav_list"),
  webdavImport: (name: string, mode: "replace" | "append"): Promise<ImportSummary> =>
    invoke("webdav_import", { name, mode }),
};

/** #11 超期阈值默认值与 settings 键（PRD §5.2）。 */
export const DEFAULT_INCREMENTAL_DAYS = 7;
export const INCREMENTAL_DAYS_KEY = "check_incremental_days";

/** 读取超期阈值；非法/缺失值一律回退默认值，避免 `NaN` 传入后端。 */
export async function getIncrementalDays(): Promise<number> {
  try {
    const raw = await api.getSetting(INCREMENTAL_DAYS_KEY);
    if (raw == null || raw.trim() === "") return DEFAULT_INCREMENTAL_DAYS;
    const value = Number(raw);
    return Number.isInteger(value) && value >= 1 ? value : DEFAULT_INCREMENTAL_DAYS;
  } catch {
    return DEFAULT_INCREMENTAL_DAYS;
  }
}

/** 写入超期阈值（预览与检测使用同一份配置快照）。 */
export async function setIncrementalDays(days: number): Promise<void> {
  const safe = Number.isInteger(days) && days >= 1 ? days : DEFAULT_INCREMENTAL_DAYS;
  await api.setSetting(INCREMENTAL_DAYS_KEY, String(safe));
}

/** #9 模糊去重开关默认值与 settings 键（PRD §5.1）。 */
export const DEFAULT_FUZZY_OPTIONS: FuzzyOptions = {
  ignoreQuery: false,
  ignoreCase: true,
  ignoreTrailingSlash: true,
  ignoreFragment: true,
};
export const FUZZY_OPTIONS_KEY = "fuzzy_options";

/** 读取开关；非法/缺失值回退默认值，并补齐缺失字段（与后端 serde(default) 对齐）。 */
export async function getFuzzyOptions(): Promise<FuzzyOptions> {
  try {
    const raw = await api.getSetting(FUZZY_OPTIONS_KEY);
    if (!raw || raw.trim() === "") return { ...DEFAULT_FUZZY_OPTIONS };
    const parsed = JSON.parse(raw) as Partial<FuzzyOptions>;
    const result = { ...DEFAULT_FUZZY_OPTIONS };
    if (typeof parsed.ignoreQuery === "boolean") result.ignoreQuery = parsed.ignoreQuery;
    if (typeof parsed.ignoreCase === "boolean") result.ignoreCase = parsed.ignoreCase;
    if (typeof parsed.ignoreTrailingSlash === "boolean") result.ignoreTrailingSlash = parsed.ignoreTrailingSlash;
    if (typeof parsed.ignoreFragment === "boolean") result.ignoreFragment = parsed.ignoreFragment;
    return result;
  } catch {
    return { ...DEFAULT_FUZZY_OPTIONS };
  }
}

/** 写入开关快照（仅本机 SQLite settings；预览与应用共用同一份 opts）。 */
export async function setFuzzyOptions(opts: FuzzyOptions): Promise<void> {
  try {
    await api.setSetting(FUZZY_OPTIONS_KEY, JSON.stringify(opts));
  } catch {
    // 设置写入失败不应阻断预览/应用；下次打开仍可回退默认值。
  }
}

/** #14 WebDAV settings 键（5 个，凭据明文仅存本机 SQLite）。 */
export const WEBDAV_SETTINGS_KEYS = {
  url: "webdav_url",
  user: "webdav_user",
  password: "webdav_password",
  dir: "webdav_dir",
  allowHttp: "webdav_allow_http",
} as const;

/** #14 WebDAV 配置默认值：仅 https、目录为根。 */
export const DEFAULT_WEBDAV_CONFIG: WebDavConfig = {
  url: "",
  user: "",
  password: "",
  dir: "/",
  allowHttp: false,
};

/** 从本机 settings 读取 WebDAV 配置；缺省/异常回退默认值。 */
export async function getWebDavConfig(): Promise<WebDavConfig> {
  try {
    const [url, user, password, dir, allowHttp] = await Promise.all([
      api.getSetting(WEBDAV_SETTINGS_KEYS.url),
      api.getSetting(WEBDAV_SETTINGS_KEYS.user),
      api.getSetting(WEBDAV_SETTINGS_KEYS.password),
      api.getSetting(WEBDAV_SETTINGS_KEYS.dir),
      api.getSetting(WEBDAV_SETTINGS_KEYS.allowHttp),
    ]);
    return {
      url: url ?? "",
      user: user ?? "",
      password: password ?? "",
      dir: dir && dir.trim() !== "" ? dir : "/",
      allowHttp: allowHttp === "1",
    };
  } catch {
    return { ...DEFAULT_WEBDAV_CONFIG };
  }
}

/** 写入 5 个 webdav_* 键；先测后存由调用方保证。 */
export async function saveWebDavConfig(cfg: WebDavConfig): Promise<void> {
  await Promise.all([
    api.setSetting(WEBDAV_SETTINGS_KEYS.url, cfg.url.trim()),
    api.setSetting(WEBDAV_SETTINGS_KEYS.user, cfg.user.trim()),
    api.setSetting(WEBDAV_SETTINGS_KEYS.password, cfg.password),
    api.setSetting(WEBDAV_SETTINGS_KEYS.dir, cfg.dir.trim() || "/"),
    api.setSetting(WEBDAV_SETTINGS_KEYS.allowHttp, cfg.allowHttp ? "1" : "0"),
  ]);
}

export type { CheckFinishedEvent };
