<template>
  <div class="main-view">
    <!-- 顶部工具栏 -->
    <div class="toolbar">
      <n-space :size="8" align="center" :wrap-item="false">
        <n-dropdown :options="importOptions" trigger="click" @select="onImportSelect">
          <n-button type="primary" ghost>{{ t("importMenu") }}</n-button>
        </n-dropdown>
        <n-button @click="onExport">{{ t("exportBtn") }}</n-button>
        <n-button @click="openCreateFolder">{{ t("newFolder") }}</n-button>
      </n-space>
      <n-space :size="8" align="center" :wrap-item="false">
        <n-button type="primary" :disabled="store.running" @click="checkAll">
          {{ t("checkAll") }}
        </n-button>
        <n-button :disabled="store.running" @click="checkSelected">{{ t("checkSelected") }}</n-button>
        <n-button :disabled="!store.running" type="warning" ghost @click="store.cancelCheck()">
          {{ t("stopCheck") }}
        </n-button>
        <n-button :disabled="store.running" @click="onIncrementalCheck">
          {{ t("incrementalCheck") }}
        </n-button>
      </n-space>
      <n-space :size="8" align="center" :wrap-item="false">
        <n-dropdown :options="organizeOptions" trigger="click" @select="onOrganizeSelect">
          <n-button>{{ t("organizeTools") }}</n-button>
        </n-dropdown>
        <n-button @click="openDedup">{{ t("dedup") }}</n-button>
        <n-button @click="openEmpty">{{ t("emptyFolders") }}</n-button>
        <n-button type="error" ghost @click="openDead">{{ t("deleteDead") }}</n-button>
        <n-badge :value="store.stats?.trashCount ?? 0" :max="99">
          <n-button @click="showTrash = true">{{ t("trash") }}</n-button>
        </n-badge>
        <n-button @click="showSettings = true">{{ t("settings") }}</n-button>
      </n-space>
    </div>

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
            :data="treeData"
            :pattern="pattern"
            :checked-keys="store.checkedIds"
            :allow-drop="allowDrop"
            :node-props="treeNodeProps"
            :render-suffix="renderSuffix"
            :render-label="renderLabel"
            :expand-on-click="false"
            key-field="key"
            label-field="label"
            children-field="children"
            @update:checked-keys="onChecked"
            @dragstart="onTreeDragStart"
            @dragend="onTreeDragEnd"
            @drop="handleDrop"
          />
          <div v-if="store.nodes.length === 0" class="empty-tip">{{ t("noBookmarks") }}</div>
        </div>
        <div class="checked-bar" v-if="store.checkedIds.length > 0">
          {{ t("checkedCount", { n: store.checkedIds.length }) }}
        </div>
      </div>

      <!-- 检测面板 -->
      <div class="check-panel">
        <div class="panel-title">{{ t("checkProgressLabel") }}</div>
        <n-progress
          type="line"
          :percentage="pct"
          :processing="store.running"
          :show-indicator="true"
        />
        <div class="counter-grid">
          <div class="counter"><span class="dot ok"></span>{{ t("statOk") }} <b>{{ store.progress.ok }}</b></div>
          <div class="counter"><span class="dot susp"></span>{{ t("statSuspicious") }} <b>{{ store.progress.suspicious }}</b></div>
          <div class="counter"><span class="dot dead"></span>{{ t("statDead") }} <b>{{ store.progress.dead }}</b></div>
          <div class="counter">{{ store.progress.done }} / {{ store.progress.total }}</div>
        </div>
        <div class="last-url" :title="store.lastUrl">{{ store.lastUrl }}</div>
        <n-alert type="info" :bordered="false" class="disclaimer">{{ t("checkDisclaimer") }}</n-alert>
      </div>
    </div>

    <!-- 底部状态栏 -->
    <div class="status-bar">
      <span>{{ t("statBookmarks") }} {{ store.stats?.bookmarks ?? "-" }}</span>
      <span>{{ t("statFolders") }} {{ store.stats?.folders ?? "-" }}</span>
      <span class="c-ok">{{ t("statOk") }} {{ store.stats?.ok ?? 0 }}</span>
      <span class="c-susp">{{ t("statSuspicious") }} {{ store.stats?.suspicious ?? 0 }}</span>
      <span class="c-dead">{{ t("statDead") }} {{ store.stats?.dead ?? 0 }}</span>
      <span class="c-gray">{{ t("statUnchecked") }} {{ store.stats?.unchecked ?? 0 }}</span>
      <span class="spacer"></span>
      <span class="db" :title="store.dbPath">{{ t("dbPath") }}: {{ store.dbPath }}</span>
    </div>

    <!-- 对话框 -->
    <DedupDialog v-model:show="showDedup" :groups="dedupGroups" @applied="onDedupApplied" />
    <EmptyFolderDialog v-model:show="showEmpty" :items="emptyItems" @applied="onEmptyApplied" />
    <DeadDialog v-model:show="showDead" @deleted="onDeadDeleted" />
    <TrashDialog v-model:show="showTrash" @changed="store.refresh()" />
    <SettingsDialog v-model:show="showSettings" />

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
  NAlert,
  NBadge,
  NButton,
  NDropdown,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NProgress,
  NSelect,
  NSpace,
  NTag,
  NTree,
  useMessage,
  type TreeOption,
} from "naive-ui";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { CheckFinishedEvent, CheckProgressEvent, DupGroup, ItemDto, TreeNode } from "../types";
import {
  bookmarkIdsUnder,
  collectSubtree,
  contextIds,
  folderOptions as buildFolderOptions,
  isRootNode,
  rootFolderId,
} from "../composables/tree";
import DedupDialog from "./DedupDialog.vue";
import EmptyFolderDialog from "./EmptyFolderDialog.vue";
import DeadDialog from "./DeadDialog.vue";
import TrashDialog from "./TrashDialog.vue";
import SettingsDialog from "./SettingsDialog.vue";

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

function statusTag(status: string, count?: number, key?: number) {
  const conf = STATUS_TAG[status];
  if (!conf) return null;
  const label = count == null ? conf.text() : `${conf.text()} ${count}`;
  return h(
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
    return st ? statusTag(st) : null;
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
const importOptions = computed(() => [
  { label: t("importReplace"), key: "replace" },
  { label: t("importAppend"), key: "append" },
]);

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
const pct = computed(() =>
  store.progress.total > 0
    ? Math.round((store.progress.done / store.progress.total) * 100)
    : 0
);

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
const dedupGroups = ref<DupGroup[]>([]);
async function openDedup() {
  dedupGroups.value = await api.dedupPreview();
  showDedup.value = true;
}
async function onDedupApplied(n: number) {
  message.success(t("dedupApplyOk", { n }));
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

// ---------------- 整理工具下拉（T02-T04 接入真实对话框） ----------------
const organizeOptions = computed(() => [
  { label: t("dedup"), key: "exactDedup" },
  { label: t("fuzzyDedup"), key: "fuzzyDedup" },
  { label: t("mergeFolders"), key: "mergeFolders" },
  { label: t("updateRedirects"), key: "updateRedirects" },
  { label: t("rulesClassify"), key: "rulesClassify" },
]);

function onOrganizeSelect(key: string) {
  if (key === "exactDedup") {
    void openDedup();
    return;
  }
  // T02-T04 会在此挂载真实对话框；当前给出明确反馈，避免点击后静默无响应。
  message.info(t("comingSoon"));
}

async function onIncrementalCheck() {
  // T02 会在此打开增量预览确认卡；当前先占位反馈。
  message.info(t("comingSoon"));
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
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid rgba(128, 128, 128, 0.18);
  flex-wrap: wrap;
}
.toolbar > :nth-child(2) {
  flex: 1;
  justify-content: center;
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
  padding: 10px;
  border-right: 1px solid rgba(128, 128, 128, 0.18);
}
.search {
  margin-bottom: 8px;
}
.tree-wrap {
  flex: 1;
  overflow: auto;
  position: relative;
}
.empty-tip {
  position: absolute;
  top: 40px;
  left: 0;
  right: 0;
  text-align: center;
  opacity: 0.55;
  font-size: 13px;
}
.checked-bar {
  padding: 6px 4px;
  font-size: 12px;
  opacity: 0.75;
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
.check-panel {
  flex: 1;
  min-width: 260px;
  max-width: 420px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.panel-title {
  font-weight: 600;
  font-size: 14px;
}
.counter-grid {
  display: flex;
  gap: 14px;
  font-size: 13px;
  align-items: center;
  flex-wrap: wrap;
}
.counter b {
  font-size: 15px;
}
.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-right: 5px;
}
.dot.ok {
  background: #22c55e;
}
.dot.susp {
  background: #f59e0b;
}
.dot.dead {
  background: #ef4444;
}
.last-url {
  font-size: 11px;
  opacity: 0.6;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-height: 14px;
}
.disclaimer {
  font-size: 12px;
}
.status-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 14px;
  font-size: 12px;
  border-top: 1px solid rgba(128, 128, 128, 0.18);
  opacity: 0.85;
}
.status-bar .spacer {
  flex: 1;
}
.status-bar .db {
  max-width: 380px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.c-ok {
  color: #22c55e;
}
.c-susp {
  color: #f59e0b;
}
.c-dead {
  color: #ef4444;
}
.c-gray {
  opacity: 0.6;
}
</style>
