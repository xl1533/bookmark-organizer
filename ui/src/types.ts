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
  /** 分组用哈希（与 #11/#12 的严格 hash 用途隔离） */
  hash: string;
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

// ---------------- 第 2 期新增类型（与 commands.rs DTO 一一对应） ----------------

/** #9 模糊去重开关（与后端 FuzzyOptions camelCase 字段一致）。 */
export interface FuzzyOptions {
  ignoreQuery: boolean;
  ignoreCase: boolean;
  ignoreTrailingSlash: boolean;
  ignoreFragment: boolean;
}

/** #11 增量检测计划：四类原因计数与实际待检测总数。 */
export interface IncrementalPlanDto {
  newCount: number;
  urlChangedCount: number;
  staleCount: number;
  skippedCount: number;
  total: number;
}

/** #10 同名文件夹分组预览。 */
export interface MergeGroupDto {
  key: string;
  folders: ItemDto[];
  totalBookmarks: number;
  suggestedKeepId: number;
}

/** #10 用户确认的分组选择。 */
export interface MergeChoiceDto {
  keepId: number;
  mergeIds: number[];
}

/** #10 合并结果（撤销靠备份重新导入，不宣称回收站可撤销）。 */
export interface MergeResultDto {
  backupPath: string;
  mergedGroups: number;
  movedNodes: number;
}

/** #12 重定向候选（旧地址 → 最终地址）。 */
export interface RedirectDto {
  nodeId: number;
  title: string;
  oldUrl: string;
  newUrl: string;
  status: CheckStatus;
  checkedAt: number;
}

/** #12 重定向应用结果。 */
export interface RedirectApplyDto {
  backupPath: string;
  updated: number;
}

/** #13 规则匹配字段。 */
export type RuleField = "host" | "urlKeyword" | "titleKeyword";

/** #13 规则（id 由后端生成）。 */
export interface Rule {
  id: string;
  field: RuleField;
  pattern: string;
  targetPath: string;
  enabled: boolean;
}

/** #13 新增规则入参（无 id）。 */
export interface RuleInput {
  field: RuleField;
  pattern: string;
  targetPath: string;
  enabled: boolean;
}

/** #13 归类预览行。 */
export interface ClassifyItemDto {
  nodeId: number;
  title: string;
  currentPath: string;
  matchedRuleId: string;
  matchedRulePattern: string;
  targetPath: string;
  conflict: boolean;
}

/** #13 用户勾选的归类项。 */
export interface ClassifyChoiceDto {
  nodeId: number;
  targetPath: string;
}

/** #14 WebDAV 连接配置（先测后存，凭据仅存本机）。 */
export interface WebDavConfig {
  url: string;
  user: string;
  password: string;
  dir: string;
  allowHttp: boolean;
}

/** #14 远端备份文件。 */
export interface WebDavFileDto {
  name: string;
  size: number;
  modified: number;
}

/** #14 上传结果。 */
export interface WebDavUploadDto {
  backupPath: string;
  remotePath: string;
  bytes: number;
}
