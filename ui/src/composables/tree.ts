import type { TreeNode } from "../types";

/**
 * 树形选项的最小结构。
 *
 * 既兼容 MainView 中基于 `TreeOption` 的 `NodeItem`（含 `node` / `children`），
 * 也便于各对话框自行构造同形数组；本模块全部为纯函数，不读取任何组件或 store 状态。
 */
export interface TreeItemLike {
  node: TreeNode;
  children: TreeItemLike[];
}

/** `n-select` 的文件夹候选项。 */
export interface FolderOption {
  label: string;
  value: number;
}

/** 根节点判据：父节点指向自身（与后端 ROOT_ID 自引用一致）。 */
export function isRootNode(n: TreeNode): boolean {
  return n.parentId === n.id;
}

/** 内部非泛型实现，避免递归调用时泛型参数 T 无法协变的问题。 */
function findItemInternal(id: number, items: readonly TreeItemLike[]): TreeItemLike | undefined {
  for (const it of items) {
    if (it.node.id === id) return it;
    const found = findItemInternal(id, it.children);
    if (found) return found;
  }
  return undefined;
}

/** 在树形选项中按节点 id 深度优先查找。 */
export function findItem<T extends TreeItemLike = TreeItemLike>(
  id: number,
  items: readonly T[]
): T | undefined {
  return findItemInternal(id, items) as T | undefined;
}

/** 收集节点自身及其整棵子树的 id（用于拖拽禁止落入自身子树等场景）。 */
export function collectSubtree(
  id: number,
  out: Set<number>,
  items: readonly TreeItemLike[]
): void {
  const item = findItemInternal(id, items);
  if (!item) return;
  const walk = (it: TreeItemLike): void => {
    out.add(it.node.id);
    for (const child of it.children) walk(child);
  };
  walk(item);
}

/** 收集节点子树内全部书签的 id（文件夹本身不计入）。 */
export function bookmarkIdsUnder(id: number, items: readonly TreeItemLike[]): number[] {
  const out: number[] = [];
  const item = findItemInternal(id, items);
  if (!item) return out;
  const walk = (it: TreeItemLike): void => {
    if (it.node.kind === "bookmark") out.push(it.node.id);
    for (const child of it.children) walk(child);
  };
  walk(item);
  return out;
}

/** 根节点（书签栏）的 id；不存在时返回 null。 */
export function rootFolderId(nodes: readonly TreeNode[]): number | null {
  return nodes.find((n) => n.kind === "folder" && isRootNode(n))?.id ?? null;
}

/**
 * 全部文件夹（含根）的 `n-select` 候选项，按展示路径排序。
 *
 * 路径中的“书签栏（根目录）”标签由调用方以 `rootLabel` 注入（通常传 `t("rootFolder")`），
 * 从而保持本模块与 i18n 无耦合。
 */
export function folderOptions(
  nodes: readonly TreeNode[],
  rootLabel: string
): FolderOption[] {
  const folders = nodes.filter((n) => n.kind === "folder");
  const pathOf = (n: TreeNode): string => {
    if (isRootNode(n)) return rootLabel;
    const parts: string[] = [n.title];
    let cur = n;
    let guard = 0;
    while (guard++ < 100) {
      const parent = folders.find((f) => f.id === cur.parentId);
      if (!parent) break;
      if (isRootNode(parent)) break; // 根用固定标签表示，不拼它的 title
      parts.unshift(parent.title);
      cur = parent;
    }
    return `${rootLabel} / ${parts.join(" / ")}`;
  };
  return folders
    .map((n) => ({ label: pathOf(n), value: n.id }))
    .sort((a, b) => String(a.label).localeCompare(String(b.label)));
}

/**
 * 右键菜单实际作用的 id 列表。
 *
 * 若右键的节点处于多选（>1）中，则作用于整个非根多选集；否则只作用于该节点。
 * 组件状态由调用方注入，便于对话框复用同一套语义。
 */
export function contextIds(
  node: TreeNode,
  checkedIds: readonly number[],
  nodeById: (id: number) => TreeNode | undefined
): number[] {
  if (checkedIds.length > 1 && checkedIds.includes(node.id)) {
    return checkedIds.filter((id) => {
      const n = nodeById(id);
      return n != null && !isRootNode(n);
    });
  }
  return [node.id];
}
