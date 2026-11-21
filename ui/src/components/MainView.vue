<template>
  <div class="main-view">
    <!-- 顶部工具栏 -->
    <div class="toolbar">
      <n-space :size="8" align="center" :wrap-item="false">
        <n-dropdown :options="importOptions" trigger="click" @select="onImportSelect">
          <n-button type="primary" ghost>{{ t("importMenu") }}</n-button>
        </n-dropdown>
        <n-button @click="onExport">{{ t("exportBtn") }}</n-button>
      </n-space>
      <n-space :size="8" align="center" :wrap-item="false">
        <n-button type="primary" :disabled="store.running" @click="checkAll">
          {{ t("checkAll") }}
        </n-button>
        <n-button :disabled="store.running" @click="checkSelected">{{ t("checkSelected") }}</n-button>
        <n-button :disabled="!store.running" type="warning" ghost @click="store.cancelCheck()">
          {{ t("stopCheck") }}
        </n-button>
      </n-space>
      <n-space :size="8" align="center" :wrap-item="false">
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
            :expand-on-click="false"
            key-field="key"
            label-field="label"
            children-field="children"
            @update:checked-keys="onChecked"
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
  </div>
</template>

<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, ref } from "vue";
import {
  NAlert,
  NBadge,
  NButton,
  NDropdown,
  NInput,
  NProgress,
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

function renderSuffix({ option }: { option: TreeOption }) {
  const item = option as NodeItem;
  if (item.node.kind !== "bookmark") return null;
  const st = store.effectiveStatus(item.node);
  if (!st) return null;
  const map = {
    ok: { label: t("statusOk"), type: "success" as const },
    suspicious: { label: t("statusSuspicious"), type: "warning" as const },
    dead: { label: t("statusDead"), type: "error" as const },
  };
  const conf = map[st];
  return h(
    NTag,
    { size: "tiny", type: conf.type, bordered: false, style: "margin-left: 8px" },
    { default: () => conf.label }
  );
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

// ---------------- 拖拽移动 ----------------
function allowDrop({
  dropPosition,
  node,
}: {
  dropPosition: "before" | "after" | "inside";
  node: TreeOption;
}) {
  const item = node as NodeItem;
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
  const dragId = dragNode.key as number;
  const target = node as NodeItem;
  const ids =
    store.checkedIds.length > 0 && store.checkedIds.includes(dragId)
      ? [...store.checkedIds]
      : [dragId];
  let targetId: number;
  let index: number;
  if (dropPosition === "inside") {
    targetId = target.key as number;
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

// ---------------- 事件监听 ----------------
let unlisten: UnlistenFn[] = [];
onMounted(async () => {
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
