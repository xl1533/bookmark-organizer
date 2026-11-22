<template>
  <div class="main-view">
    <!-- 顶部工具栏 -->
    <MainToolbar
      :running="store.running"
      :trash-count="store.stats?.trashCount ?? 0"
      @import-select="onImportSelect"
      @export="onExport"
      @create-folder="openCreateFolder"
      @check-all="checkAll"
      @check-selected="checkSelected"
      @stop-check="store.cancelCheck()"
      @incremental-check="onIncrementalCheck"
      @organize-select="onOrganizeSelect"
      @dedup="openDedup"
      @empty="openEmpty"
      @dead="openDead"
      @trash="showTrash = true"
      @settings="showSettings = true"
    />

    <!-- 主体区域 -->
    <div class="body">
      <div class="tree-panel">
        <n-input
          v-model:value="pattern"
          :placeholder="t('searchPlaceholder')"
          clearable
          size="small"
          class="search"
        />
        <div class="tree-wrap">
          <n-tree
            block-line
            checkable
            cascade
            draggable
            virtual-scroll
            :data="treeData"
            :pattern="pattern"
            :checked-keys="store.checkedIds"
            :allow-drop="allowDrop"
            :node-props="treeNodeProps"
            :render-suffix="renderSuffix"
            :render-label="renderLabel"
            :expand-on-click="false"
            style="height: 100%"
            key-field="key"
            label-field="label"
            children-field="children"
            @update:checked-keys="onChecked"
            @dragstart="onTreeDragStart"
            @dragend="onTreeDragEnd"
            @drop="handleDrop"
          />
          <div v-if="store.nodes.length === 0" class="empty-tip">
            <div class="empty-card">
              <span class="empty-mark" aria-hidden="true"></span>
              <span>{{ t("noBookmarks") }}</span>
            </div>
          </div>
        </div>
        <div class="checked-bar" v-if="store.checkedIds.length > 0">
          {{ t("checkedCount", { n: store.checkedIds.length }) }}
        </div>
      </div>

      <!-- 检测面板 -->
      <CheckPanel
        :running="store.running"
        :progress="store.progress"
        :last-url="store.lastUrl"
      />
    </div>

    <!-- 底部状态栏 -->
    <StatusBar :stats="store.stats" :db-path="store.dbPath" />

    <!-- 对话框 -->
    <DedupDialog
      v-model:show="showDedup"
      :mode="dedupMode"
      :groups="dedupGroups"
      @applied="onDedupApplied"
    />
    <EmptyFolderDialog v-model:show="showEmpty" :items="emptyItems" @applied="onEmptyApplied" />
    <DeadDialog v-model:show="showDead" @deleted="onDeadDeleted" />
    <TrashDialog v-model:show="showTrash" @changed="store.refresh()" />
    <SettingsDialog v-model:show="showSettings" />
    <IncrementDialog
      v-model:show="showIncrement"
      @started="onIncrementStarted"
      @go-settings="onIncrementGoSettings"
    />
    <RedirectDialog v-model:show="showRedirect" @applied="onRedirectApplied" />
    <MergeFoldersDialog v-model:show="showMergeFolders" @applied="onMergeApplied" />
    <RulesDialog v-model:show="showRules" @preview="onRulesPreview" />
    <ClassifyPreviewDialog
      v-model:show="showClassifyPreview"
      :selected-ids="store.checkedIds"
      @applied="onClassifyApplied"
    />

    <!-- 树节点右键菜单（浏览器原生菜单已在 main.ts 中屏蔽） -->
    <n-dropdown
      trigger="manual"
      :show="ctxMenu.show"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :options="ctxOptions"
      @select="onCtxSelect"
      @update:show="(v) => (ctxMenu.show = v)"
    />

    <!-- 右键「移动到…」 -->
    <n-modal
      v-model:show="showMoveModal"
      preset="card"
      :title="t('moveTitle', { n: moveIds.length })"
      style="width: 440px"
    >
      <n-form label-placement="left" label-width="90">
        <n-form-item :label="t('moveTarget')">
          <n-select v-model:value="moveTargetId" :options="moveOptions" />
        </n-form-item>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showMoveModal = false">{{ t("cancel") }}</n-button>
          <n-button type="primary" :loading="moving" @click="doMove">
            {{ t("moveConfirm") }}
          </n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 右键「删除」 -->
    <n-modal
      v-model:show="showDeleteModal"
      preset="card"
      :title="t('deleteTitle', { n: deleteIds.length })"
      style="width: 460px"
    >
      <p class="delete-hint">{{ t("deleteHint") }}</p>
      <ul class="delete-list">
        <li v-for="(name, i) in deleteNames" :key="i">{{ name }}</li>
      </ul>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showDeleteModal = false">{{ t("cancel") }}</n-button>
          <n-button type="error" ghost :loading="deleting" @click="doDelete">
            {{ t("ctxDelete") }}
          </n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 新建文件夹 -->
    <n-modal
      v-model:show="showCreateFolder"
      preset="card"
      :title="t('newFolderTitle')"
      style="width: 440px"
    >
      <p class="create-hint">{{ t("createdInHint", { name: parentFolderName }) }}</p>
      <n-form label-placement="left" label-width="90">
        <n-form-item :label="t('folderName')">
          <n-input
            v-model:value="newFolderName"
            :placeholder="t('folderName')"
            autofocus
            @keyup.enter="doCreateFolder"
          />
        </n-form-item>
        <n-form-item :label="t('parentFolder')">
          <n-select v-model:value="createParentId" :options="folderOptions" />
        </n-form-item>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showCreateFolder = false">{{ t("cancel") }}</n-button>
          <n-button type="primary" :loading="creatingFolder" @click="doCreateFolder">
            {{ t("confirm") }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, reactive, ref } from "vue";
import {
  NButton,
  NDropdown,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NTag,
  NTooltip,
  NTree,
  useMessage,
  type TreeOption,
} from "naive-ui";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { useI18n } from "../i18n";
import { api } from "../api";
import type {
  CheckFinishedEvent,
  CheckProgressEvent,
  DupGroup,
  ItemDto,
  MergeResultDto,
  TreeNode,
} from "../types";
import {
  bookmarkIdsUnder,
  collectSubtree,
  contextIds,
  folderOptions as buildFolderOptions,
  isRootNode,
  rootFolderId,
} from "../composables/tree";
import MainToolbar from "./MainToolbar.vue";
import CheckPanel from "./CheckPanel.vue";
import StatusBar from "./StatusBar.vue";
import DedupDialog from "./DedupDialog.vue";
import EmptyFolderDialog from "./EmptyFolderDialog.vue";
import DeadDialog from "./DeadDialog.vue";
import TrashDialog from "./TrashDialog.vue";
import SettingsDialog from "./SettingsDialog.vue";
import IncrementDialog from "./IncrementDialog.vue";
import RedirectDialog from "./RedirectDialog.vue";
import MergeFoldersDialog from "./MergeFoldersDialog.vue";
import RulesDialog from "./RulesDialog.vue";
import ClassifyPreviewDialog from "./ClassifyPreviewDialog.vue";

const store = useAppStore();
const { t } = useI18n();
const message = useMessage();

// ---------------- 树 ----------------
interface NodeItem extends TreeOption {
  key: number;
  label: string;
  isLeaf: boolean;
  children: NodeItem[];
  node: TreeNode;
}

const STATUS_TAG: Record<string, { text: () => string; type: "success" | "warning" | "error" }> = {
  ok: { text: () => t("statusOk"), type: "success" },
  suspicious: { text: () => t("statusSuspicious"), type: "warning" },
  dead: { text: () => t("statusDead"), type: "error" },
};

function statusTag(status: string, count?: number, key?: number, detail?: string) {
  const conf = STATUS_TAG[status];
  if (!conf) return null;
  const label = count == null ? conf.text() : `${conf.text()} ${count}`;
  const tag = h(
    NTag,
    {
      size: "tiny",
      type: conf.type,
      bordered: false,
      style: "margin-left: 8px",
      ...(key != null ? { key } : {}),
    },
    { default: () => label }
  );
  // 书签状态附详情提示（状态码 + 原因），帮助用户判断可疑/失效的具体原因
  if (detail) {
    return h(
      NTooltip,
      { placement: "top", style: "margin-left: 8px" },
      {
        trigger: () => tag,
        default: () => detail,
      }
    );
  }
  return tag;
}

// 从检测结果提取状态详情文本（HTTP 状态码 + 原因）
function checkDetail(node: TreeNode): string | undefined {
  const c = node.check;
  if (!c) return undefined;
  const parts: string[] = [];
  if (c.httpCode != null) parts.push(`HTTP ${c.httpCode}`);
  if (c.message) parts.push(c.message);
  if (c.finalUrl && c.finalUrl !== node.url) parts.push(`→ ${c.finalUrl}`);
  return parts.length ? parts.join(" · ") : undefined;
}

// 点击书签名称用默认浏览器打开；文件夹名称保持普通文本（点击仅展开）
function renderLabel({ option }: { option: TreeOption }) {
  const item = option as NodeItem;
  if (item.node.kind !== "bookmark") return item.label;
  return h(
    "span",
    {
      class: "link-label",
      title: t("openInBrowser"),
      onClick: (e: MouseEvent) => {
        e.stopPropagation();
        void openBookmark(item.node);
      },
    },
    item.label
  );
}

async function openBookmark(node: TreeNode) {
  if (!node.url) {
    message.warning(t("noUrlToOpen"));
    return;
  }
  try {
    await api.openUrl(node.url);
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  }
}

// n-tree 的 render-suffix 是组件级 prop；挂在每个 option 上的 renderSuffix 字段不会被渲染
function renderSuffix({ option }: { option: TreeOption }) {
  const item = option as NodeItem;
  if (item.node.kind === "bookmark") {
    const st = store.effectiveStatus(item.node);
    return st ? statusTag(st, undefined, undefined, checkDetail(item.node)) : null;
  }
  // 文件夹：聚合子树内各状态数量，便于在树中一眼看到检测结果
  const counts = subtreeStatusCounts(item);
  const tags = [];
  if (counts.dead > 0) tags.push(statusTag("dead", counts.dead, 1));
  if (counts.suspicious > 0) tags.push(statusTag("suspicious", counts.suspicious, 2));
  if (counts.ok > 0) tags.push(statusTag("ok", counts.ok, 3));
  return tags.length ? h("span", { style: "display:inline-flex;gap:0" }, tags) : null;
}

function subtreeStatusCounts(item: NodeItem) {
  const counts = { ok: 0, suspicious: 0, dead: 0 };
  const walk = (n: NodeItem) => {
    if (n.node.kind === "bookmark") {
      const st = store.effectiveStatus(n.node);
      if (st) counts[st as keyof typeof counts]++;
    }
    n.children.forEach(walk);
  };
  walk(item);
  return counts;
}

const treeData = computed<NodeItem[]>(() => {
  const map = new Map<number, NodeItem>();
  for (const n of store.nodes) {
    map.set(n.id, {
      key: n.id,
      label: n.title,
      isLeaf: n.kind === "bookmark",
      children: [],
      node: n,
      renderSuffix: n.kind === "bookmark" ? renderSuffix : undefined,
    } as unknown as NodeItem);
  }
  const roots: NodeItem[] = [];
  for (const n of store.nodes) {
    const item = map.get(n.id);
    if (!item) continue;
    if (n.parentId === n.id) {
      roots.push(item);
    } else {
      map.get(n.parentId)?.children.push(item);
    }
  }
  const sortRec = (items: NodeItem[]) => {
    items.sort((a, b) => a.node.position - b.node.position);
    for (const it of items) sortRec(it.children);
  };
  sortRec(roots);
  return roots;
});

const pattern = ref("");

function onChecked(keys: number[]) {
  store.checkedIds = keys;
}

// ---------------- 树节点公共工具 ----------------
// isRootNode / findItem / collectSubtree / bookmarkIdsUnder / rootFolderId /
// folderOptions / contextIds 已抽到 ../composables/tree，供各对话框复用；
// 本组件只在调用处注入 treeData.value / store.nodes / store.checkedIds 等响应式状态。

// ---------------- 拖拽移动 ----------------
const dragIds = ref<number[]>([]);
const forbiddenDropIds = ref<Set<number>>(new Set());

function onTreeDragStart({ event, node }: { event: DragEvent; node: TreeOption }) {
  const id = (node as NodeItem).node.id;
  // 某些 WebView 内核要求 dragstart 必须有 dataTransfer 数据才会触发后续 dragover/drop
  event.dataTransfer?.setData("text/plain", String(id));
  // 拖拽已勾选的节点时，连同其他勾选节点一起移动
  dragIds.value =
    store.checkedIds.length > 0 && store.checkedIds.includes(id)
      ? [...store.checkedIds]
      : [id];
  const forbidden = new Set<number>();
  for (const dragId of dragIds.value) collectSubtree(dragId, forbidden, treeData.value);
  forbiddenDropIds.value = forbidden;
}

function onTreeDragEnd() {
  dragIds.value = [];
  forbiddenDropIds.value = new Set();
}

function allowDrop({
  dropPosition,
  node,
}: {
  dropPosition: "before" | "after" | "inside";
  node: TreeOption;
}) {
  const item = node as NodeItem;
  if (forbiddenDropIds.value.has(item.node.id)) return false;
  if (dropPosition === "inside") return item.node.kind === "folder";
  return true;
}

async function handleDrop({
  node,
  dragNode,
  dropPosition,
}: {
  node: TreeOption;
  dragNode: TreeOption;
  dropPosition: "before" | "after" | "inside";
}) {
  const dragId = (dragNode as NodeItem).node.id;
  const ids = dragIds.value.includes(dragId) ? [...dragIds.value] : [dragId];
  const target = node as NodeItem;
  if (ids.some((id) => forbiddenDropIds.value.has(target.node.id))) {
    message.warning(t("moveErrorOwnSubtree"));
    onTreeDragEnd();
    return;
  }
  let targetId: number;
  let index: number;
  if (dropPosition === "inside") {
    targetId = target.node.id;
    index = 99999; // 追加到末尾（后端会做范围裁剪）
  } else {
    targetId = target.node.parentId;
    index = dropPosition === "before" ? target.node.position : target.node.position + 1;
  }
  try {
    await api.moveNodes(ids, targetId, index);
    message.success(t("movedOk", { n: ids.length }));
    store.checkedIds = [];
    await store.refresh();
  } catch (e) {
    message.error(String(e));
  } finally {
    onTreeDragEnd();
  }
}

// ---------------- 右键菜单 ----------------
type CtxOption = { label: string; key: string; disabled?: boolean };

const ctxMenu = reactive({ show: false, x: 0, y: 0, node: null as TreeNode | null });

function onDocClick() {
  if (ctxMenu.show) ctxMenu.show = false;
}

function treeNodeProps({ option }: { option: TreeOption }) {
  return {
    onContextmenu: (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      ctxMenu.node = (option as NodeItem).node;
      ctxMenu.x = e.clientX;
      ctxMenu.y = e.clientY;
      ctxMenu.show = true;
    },
  };
}

const ctxOptions = computed<CtxOption[]>(() => {
  const node = ctxMenu.node;
  if (!node) return [];
  const root = isRootNode(node);
  const opts: CtxOption[] = [];
  if (node.kind === "bookmark") {
    opts.push({ label: t("ctxOpen"), key: "open" });
  } else {
    opts.push({ label: t("ctxNewChild"), key: "newChild" });
  }
  opts.push({ label: t("ctxCheck"), key: "check" });
  opts.push({ label: t("ctxMove"), key: "move", disabled: root });
  opts.push({ label: t("ctxDelete"), key: "delete", disabled: root });
  return opts;
});

/** 把 store 的响应式状态注入纯函数 contextIds，保持第 1 期右键作用域语义。 */
function contextIdsForNode(node: TreeNode): number[] {
  return contextIds(node, store.checkedIds, (id) => store.nodeById(id));
}

async function onCtxSelect(key: string) {
  const node = ctxMenu.node;
  ctxMenu.show = false;
  if (!node) return;
  switch (key) {
    case "open":
      await openBookmark(node);
      break;
    case "check": {
      const ids = node.kind === "bookmark" ? [node.id] : bookmarkIdsUnder(node.id, treeData.value);
      if (ids.length === 0) {
        message.info(t("checkNothing"));
        break;
      }
      try {
        const total = await store.startCheck("selected", ids);
        message.info(total > 0 ? t("checkStarted", { n: total }) : t("checkNothing"));
      } catch (e) {
        message.error(`${t("opFailed")}: ${e}`);
      }
      break;
    }
    case "newChild":
      openCreateFolder(node.id);
      break;
    case "move":
      openMoveModal(contextIdsForNode(node), node.parentId);
      break;
    case "delete":
      openDeleteModal(contextIdsForNode(node));
      break;
  }
}

// 右键「移动到…」
const showMoveModal = ref(false);
const moveIds = ref<number[]>([]);
const moveTargetId = ref<number | null>(null);
const moving = ref(false);

const moveOptions = computed(() => {
  const forbidden = new Set<number>();
  for (const id of moveIds.value) collectSubtree(id, forbidden, treeData.value);
  return folderOptions.value.filter((o) => !forbidden.has(o.value as number));
});

function openMoveModal(ids: number[], defaultTarget: number) {
  moveIds.value = ids;
  moveTargetId.value = folderOptions.value.some((o) => o.value === defaultTarget)
    ? defaultTarget
    : (rootFolderId(store.nodes) ?? (folderOptions.value[0]?.value as number | undefined) ?? null);
  showMoveModal.value = true;
}

async function doMove() {
  if (moveTargetId.value == null) {
    message.warning(t("moveNoTarget"));
    return;
  }
  moving.value = true;
  try {
    await api.moveNodes(moveIds.value, moveTargetId.value, 99999);
    showMoveModal.value = false;
    message.success(t("movedOk", { n: moveIds.value.length }));
    store.checkedIds = [];
    await store.refresh();
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    moving.value = false;
  }
}

// 右键「删除」
const showDeleteModal = ref(false);
const deleteIds = ref<number[]>([]);
const deleting = ref(false);
const deleteNames = computed(() =>
  deleteIds.value.map((id) => store.nodeById(id)?.title ?? `#${id}`)
);

function openDeleteModal(ids: number[]) {
  deleteIds.value = ids;
  showDeleteModal.value = true;
}

async function doDelete() {
  deleting.value = true;
  try {
    const n = await api.deleteNodes(deleteIds.value);
    showDeleteModal.value = false;
    message.success(t("deletedOk", { n }));
    store.checkedIds = [];
    await store.refresh();
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    deleting.value = false;
  }
}

// ---------------- 导入 / 导出 ----------------
async function onImportSelect(mode: string) {
  const path = await openFileDialog({
    multiple: false,
    directory: false,
    filters: [{ name: t("htmlFilterName"), extensions: ["html", "htm"] }],
  });
  if (!path) return;
  try {
    const summary = await api.importHtml(path as string, mode as "replace" | "append");
    message.success(t("importedOk", { b: summary.bookmarks, f: summary.folders }));
    await store.refresh();
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  }
}

async function onExport() {
  const path = await saveFileDialog({
    defaultPath: "bookmarks.html",
    filters: [{ name: t("htmlFilterName"), extensions: ["html"] }],
  });
  if (!path) return;
  try {
    const n = await api.exportHtml(path as string);
    message.success(t("exportedOk", { n }));
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  }
}

// ---------------- 检测 ----------------
async function checkAll() {
  const total = await store.startCheck("all", []);
  if (total === 0) message.info(t("checkNothing"));
  else message.info(t("checkStarted", { n: total }));
}

async function checkSelected() {
  const ids = store.checkedIds;
  if (ids.length === 0) {
    message.warning(t("selectSomeFirst"));
    return;
  }
  const total = await store.startCheck("selected", ids);
  if (total === 0) message.info(t("checkNothing"));
  else message.info(t("checkStarted", { n: total }));
}

// ---------------- 对话框 ----------------
const showDedup = ref(false);
const dedupMode = ref<"exact" | "fuzzy">("exact");
const dedupGroups = ref<DupGroup[]>([]);
async function openDedup() {
  dedupMode.value = "exact";
  dedupGroups.value = await api.dedupPreview();
  showDedup.value = true;
}
function openFuzzyDedup() {
  dedupMode.value = "fuzzy";
  // DedupDialog 打开时自行读取 settings.fuzzy_options 并重算预览
  showDedup.value = true;
}
async function onDedupApplied(n: number) {
  message.success(
    dedupMode.value === "fuzzy" ? t("fuzzyApplyOk", { n }) : t("dedupApplyOk", { n })
  );
  await store.refresh();
}

const showMergeFolders = ref(false);
async function onMergeApplied(result: MergeResultDto) {
  message.success(t("mergeApplyOk", { n: result.mergedGroups, m: result.movedNodes }));
  await store.refresh();
}

// ---------------- T04：规则归类 ----------------
const showRules = ref(false);
const showClassifyPreview = ref(false);

function onRulesPreview(): void {
  showRules.value = false;
  showClassifyPreview.value = true;
}

async function onClassifyApplied(_count: number): Promise<void> {
  // 成功提示已在预览对话框内展示，这里只刷新树与统计。
  store.checkedIds = [];
  await store.refresh();
}

const showEmpty = ref(false);
const emptyItems = ref<ItemDto[]>([]);
async function openEmpty() {
  emptyItems.value = await api.findEmptyFolders();
  showEmpty.value = true;
}
async function onEmptyApplied(n: number) {
  message.success(t("emptyApplyOk", { n }));
  await store.refresh();
}

const showDead = ref(false);
async function openDead() {
  showDead.value = true; // DeadDialog 内部自行加载 list_dead
}

async function onDeadDeleted(n: number) {
  message.success(t("deadDeleted", { n }));
  await store.refresh();
}

const showTrash = ref(false);
const showSettings = ref(false);

// ---------------- 整理工具下拉（T03 已接入模糊去重/合并文件夹） ----------------
function onOrganizeSelect(key: string) {
  if (key === "exactDedup") {
    void openDedup();
    return;
  }
  if (key === "fuzzyDedup") {
    openFuzzyDedup();
    return;
  }
  if (key === "mergeFolders") {
    showMergeFolders.value = true;
    return;
  }
  if (key === "updateRedirects") {
    showRedirect.value = true;
    return;
  }
  if (key === "rulesClassify") {
    showRules.value = true;
    return;
  }
}

function onIncrementalCheck() {
  showIncrement.value = true;
}

// ---------------- T02：增量检测 / 重定向更新 ----------------
const showIncrement = ref(false);
const showRedirect = ref(false);

function onIncrementStarted(total: number) {
  // 后端通过 check://progress / check://finished 驱动进度，这里只初始化显示。
  // total=0 时后端不派发事件，不能把 running 置为 true，否则进度条永久卡住。
  if (total <= 0) {
    message.info(t("incrementEmpty"));
    return;
  }
  store.running = true;
  store.progress.done = 0;
  store.progress.total = total;
  store.progress.ok = 0;
  store.progress.suspicious = 0;
  store.progress.dead = 0;
  message.info(t("incrementStarted", { n: total }));
}

function onIncrementGoSettings() {
  showIncrement.value = false;
  showSettings.value = true;
}

async function onRedirectApplied(n: number) {
  message.success(t("redirectApplyOk", { n }));
  await store.refresh();
}

// ---------------- 新建文件夹 ----------------
const showCreateFolder = ref(false);
const newFolderName = ref("");
const createParentId = ref<number | null>(null);
const creatingFolder = ref(false);

// 所有文件夹（含根）作为父目录候选项；路径拼接与排序逻辑已抽到 composables/tree.ts
const folderOptions = computed(() => buildFolderOptions(store.nodes, t("rootFolder")));

const parentFolderName = computed(() => {
  const found = folderOptions.value.find((o) => o.value === createParentId.value);
  return found ? String(found.label) : "";
});

function openCreateFolder(parentId?: number) {
  newFolderName.value = "";
  if (parentId != null) {
    // 右键文件夹时直接以它为父目录，免去再选一次
    createParentId.value = parentId;
  } else {
    // 工具栏入口：默认父目录优先当前勾选的第一个文件夹，否则根目录
    const checkedFolder = store.checkedIds
      .map((id) => store.nodeById(id))
      .find((n) => n && n.kind === "folder");
    createParentId.value = checkedFolder?.id ?? rootFolderId(store.nodes) ?? null;
  }
  showCreateFolder.value = true;
}

async function doCreateFolder() {
  const name = newFolderName.value.trim();
  if (!name) {
    message.warning(t("folderNameRequired"));
    return;
  }
  if (createParentId.value == null) {
    message.error(t("opFailed"));
    return;
  }
  creatingFolder.value = true;
  try {
    await api.createFolder(createParentId.value, name, 99999);
    showCreateFolder.value = false;
    message.success(t("folderCreated"));
    await store.refresh();
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    creatingFolder.value = false;
  }
}

// ---------------- 事件监听 ----------------
let unlisten: UnlistenFn[] = [];
onMounted(async () => {
  document.addEventListener("click", onDocClick);
  await store.refresh();
  unlisten.push(
    await listen<CheckProgressEvent>("check://progress", (e) => {
      store.onCheckProgress(e.payload);
    })
  );
  unlisten.push(
    await listen<CheckFinishedEvent>("check://finished", async (e) => {
      await store.onCheckFinished(e.payload);
      if (e.payload.canceled) message.warning(t("checkCanceled"));
      else
        message.success(
          t("checkFinished", {
            ok: e.payload.ok,
            susp: e.payload.suspicious,
            dead: e.payload.dead,
          })
        );
    })
  );
});
onUnmounted(() => {
  document.removeEventListener("click", onDocClick);
  unlisten.forEach((f) => f());
});
</script>

<style scoped>
.main-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}
.body {
  flex: 1;
  display: flex;
  min-height: 0;
}
.tree-panel {
  flex: 1.6;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--bo-space-sm, 8px);
  padding: var(--bo-space-lg, 10px);
  border-right: 1px solid var(--bo-border, rgba(128, 128, 128, 0.18));
}
.tree-wrap {
  flex: 1;
  overflow: auto;
  position: relative;
  border-radius: var(--bo-radius, 8px);
}
.empty-tip {
  position: absolute;
  top: 40px;
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  pointer-events: none;
}
.empty-card {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 14px 20px;
  border: 1px dashed var(--bo-border, rgba(128, 128, 128, 0.28));
  border-radius: var(--bo-radius, 8px);
  background: var(--bo-surface-2, rgba(128, 128, 128, 0.05));
  color: var(--bo-text-muted, inherit);
  font-size: 13px;
  opacity: 0.9;
}
.empty-mark {
  flex: none;
  width: 10px;
  height: 14px;
  border-radius: 2px 2px 0 0;
  background: var(--bo-primary, #2563eb);
  opacity: 0.5;
}
.checked-bar {
  padding: 6px 10px;
  border-radius: var(--bo-radius-sm, 6px);
  background: var(--bo-surface-2, rgba(128, 128, 128, 0.08));
  font-size: 12px;
  opacity: 0.85;
}
.create-hint,
.delete-hint {
  margin: 0 0 10px;
  font-size: 12px;
  opacity: 0.7;
}
.delete-list {
  margin: 0;
  padding-left: 20px;
  max-height: 260px;
  overflow: auto;
  font-size: 13px;
  line-height: 1.8;
}
</style>
