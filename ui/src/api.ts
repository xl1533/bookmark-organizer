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

export type { CheckFinishedEvent };
