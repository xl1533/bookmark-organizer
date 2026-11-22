<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('mergeTitle')"
    style="width: 920px"
    @after-enter="load"
  >
    <n-alert type="warning" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      <div>{{ t("mergeHint") }}</div>
      <div>{{ t("mergeRecursiveNote") }}</div>
      <div>{{ t("mergeBackupNote") }}</div>
      <div>{{ t("mergeSuggestHint") }}</div>
    </n-alert>

    <template v-if="result">
      <n-result
        status="success"
        :title="t('mergeApplyOk', { n: result.mergedGroups, m: result.movedNodes })"
        size="small"
      >
        <template #default>
          <div class="backup-path">
            {{ t("mergeBackupPath") }}:
            <n-text code>{{ result.backupPath }}</n-text>
          </div>
        </template>
      </n-result>
    </template>

    <template v-else>
      <n-data-table
        :columns="columns"
        :data="groups"
        :row-key="rowKey"
        :loading="loading"
        :max-height="420"
        size="small"
      />
      <div v-if="groups.length === 0 && !loading" class="empty">{{ t("mergeEmpty") }}</div>
    </template>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          v-if="!result"
          type="primary"
          :disabled="groups.length === 0 || loading"
          :loading="applying"
          @click="apply"
        >
          {{ t("apply") }}
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, h, ref } from "vue";
import {
  NAlert,
  NButton,
  NDataTable,
  NModal,
  NRadio,
  NRadioGroup,
  NResult,
  NSpace,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import { useAppStore } from "../stores/app";
import type { ItemDto, MergeChoiceDto, MergeGroupDto, MergeResultDto } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "update:show", v: boolean): void;
  (e: "applied", result: MergeResultDto): void;
}>();
const { t } = useI18n();
const message = useMessage();
const app = useAppStore();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const groups = ref<MergeGroupDto[]>([]);
/** 每组用户选定的保留项；键为规范化组名，默认取后端建议（DFS 第一个）。 */
const keepByKey = ref<Record<string, number>>({});
const result = ref<MergeResultDto | null>(null);
const loading = ref(false);
const applying = ref(false);

function rowKey(row: MergeGroupDto): string {
  return row.key;
}

function directCount(folderId: number): number {
  return app.nodes.filter((n) => n.parentId === folderId).length;
}

function groupDirectCount(row: MergeGroupDto): number {
  return row.folders.reduce((sum, folder) => sum + directCount(folder.id), 0);
}

function folderLines(row: MergeGroupDto) {
  return row.folders.map((folder: ItemDto) =>
    h(
      "div",
      { key: folder.id, style: "font-size: 12px; line-height: 1.8" },
      `#${folder.id} ${t("mergeColPath")}: ${folder.path || folder.title}（${t("mergeColDirect")} ${directCount(folder.id)}）`
    )
  );
}

const columns = computed<DataTableColumns<MergeGroupDto>>(() => [
  {
    type: "expand",
    renderExpand: (row) =>
      h("div", { style: "padding: 4px 0 8px 12px" }, folderLines(row)),
  },
  {
    title: t("mergeColGroup"),
    key: "key",
    width: 160,
    render: (row) =>
      h("span", { style: "font-size: 12px" }, row.folders[0]?.title ?? row.key),
  },
  {
    title: t("mergeColFolders"),
    key: "folders",
    width: 90,
    render: (row) => h("span", String(row.folders.length)),
  },
  {
    title: t("mergeColDirect"),
    key: "direct",
    width: 100,
    render: (row) => h("span", String(groupDirectCount(row))),
  },
  {
    title: t("mergeColBookmarks"),
    key: "totalBookmarks",
    width: 100,
    render: (row) => h("span", String(row.totalBookmarks)),
  },
  {
    title: t("mergeColKeep"),
    key: "keep",
    minWidth: 220,
    render: (row) =>
      h(
        NRadioGroup,
        {
          size: "small",
          value: keepByKey.value[row.key] ?? row.suggestedKeepId,
          "onUpdate:value": (value: number) => {
            keepByKey.value = { ...keepByKey.value, [row.key]: value };
          },
        },
        {
          default: () =>
            row.folders.map((folder) =>
              h(
                NRadio,
                { key: folder.id, value: folder.id },
                { default: () => `${folder.title} (#${folder.id})` }
              )
            ),
        }
      ),
  },
]);

async function load(): Promise<void> {
  result.value = null;
  loading.value = true;
  try {
    const preview = await api.mergeFoldersPreview();
    groups.value = preview;
    const defaults: Record<string, number> = {};
    for (const group of preview) defaults[group.key] = group.suggestedKeepId;
    keepByKey.value = defaults;
  } catch (e) {
    message.error(t("opFailed"));
    groups.value = [];
    keepByKey.value = {};
    console.error(e);
  } finally {
    loading.value = false;
  }
}

async function apply(): Promise<void> {
  if (applying.value || groups.value.length === 0) return;
  applying.value = true;
  try {
    const choices: MergeChoiceDto[] = groups.value.map((group) => {
      const keepId = keepByKey.value[group.key] ?? group.suggestedKeepId;
      return {
        keepId,
        mergeIds: group.folders.filter((folder) => folder.id !== keepId).map((folder) => folder.id),
      };
    });
    const applied = await api.mergeFoldersApply(choices);
    result.value = applied;
    emit("applied", applied);
  } catch (e) {
    message.error(t("opFailed"));
    console.error(e);
  } finally {
    applying.value = false;
  }
}
</script>

<style scoped>
.backup-path {
  font-size: 12px;
  word-break: break-all;
}
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
</style>
