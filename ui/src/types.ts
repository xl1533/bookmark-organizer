export type NodeKind = "folder" | "bookmark";
export type CheckStatus = "ok" | "suspicious" | "dead";

export interface CheckInfo {
  status: CheckStatus;
  httpCode: number | null;
  finalUrl: string | null;
  message: string | null;
  checkedAt: number;
}

export interface TreeNode {
  id: number;
  parentId: number;
  kind: NodeKind;
  title: string;
  url: string | null;
  addDate: number | null;
  position: number;
  check: CheckInfo | null;
}

export interface Stats {
  folders: number;
  bookmarks: number;
  ok: number;
  suspicious: number;
  dead: number;
  unchecked: number;
  trashCount: number;
}

export interface ItemDto {
  id: number;
  title: string;
  path: string;
}

export interface DupGroup {
  normalizedUrl: string;
  keep: ItemDto;
  duplicates: ItemDto[];
}

export interface DeadItem {
  id: number;
  title: string;
  url: string;
  httpCode: number | null;
  message: string | null;
  checkedAt: number;
  path: string;
}

export interface DeleteDeadResult {
  backupPath: string;
  deleted: number;
  items: DeadItem[];
}

export interface TrashEntry {
  id: number;
  nodeId: number;
  title: string;
  kind: NodeKind;
  itemCount: number;
  deletedAt: number;
}

export interface CheckProgressEvent {
  done: number;
  total: number;
  nodeId: number;
  url: string;
  status: CheckStatus;
  httpCode: number | null;
  message: string | null;
}

export interface CheckFinishedEvent {
  done: number;
  total: number;
  ok: number;
  suspicious: number;
  dead: number;
  canceled: boolean;
}

export interface ProxyTestResult {
  ok: boolean;
  latencyMs: number | null;
  message: string;
}

export interface ImportSummary {
  folders: number;
  bookmarks: number;
}
