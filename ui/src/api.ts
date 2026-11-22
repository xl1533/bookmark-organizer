import { invoke } from "@tauri-apps/api/core";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import type {
  CheckFinishedEvent,
  DeadItem,
  DeleteDeadResult,
  DupGroup,
  ImportSummary,
  ItemDto,
  ProxyTestResult,
  Stats,
  TrashEntry,
  TreeNode,
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
};

export type { CheckFinishedEvent };
