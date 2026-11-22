<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('classifyTitle')"
    style="width: 980px"
    @after-enter="load"
  >
    <n-alert type="info" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("classifyHint") }}
    </n-alert>

    <n-space size="small" align="center" style="margin-bottom: 10px">
      <n-switch v-model:value="includeClassified" size="small" @update:value="reload" />
      <n-text depth="3" style="font-size: 12px">{{ t("classifyIncludeClassified") }}</n-text>
      <n-button size="tiny" :disabled="selectableIds.length === 0" @click="selectAll">
        {{ t("redirectSelectAll") }}
      </n-button>
      <n-text depth="3" style="font-size: 12px">{{ t("classifyScopeHint") }}</n-text>
    </n-space>

    <n-data-table
      :columns="columns"
      :data="items"
      :row-key="rowKey"
      :checked-row-keys="checkedIds"
      :loading="loading"
      :max-height="420"
      size="small"
      @update:checked-row-keys="onChecked"
    />
    <div v-if="items.length === 0 && !loading" class="empty">{{ t("classifyEmpty") }}</div>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          type="primary"
          :disabled="checkedIds.length === 0"
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
  NSpace,
  NSwitch,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { api } from "../api";
import { useI18n } from "../i18n";
import type { ClassifyChoiceDto, ClassifyItemDto } from "../types";

const props = defineProps<{ show: boolean; selectedIds: number[] }>();
const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "applied", count: number): void;
}>();
const { t } = useI18n();
const message = useMessage();

const visible = computed({
  get: () => props.show,
  set: (value: boolean) => emit("update:show", value),
});

const includeClassified = ref(false);
const items = ref<ClassifyItemDto[]>([]);
const checkedIds = ref<number[]>([]);
const loading = ref(false);
const applying = ref(false);

function rowKey(row: ClassifyItemDto): number {
  return row.nodeId;
}

/** 冲突项在表格中禁用勾选，也不参与批量全选。 */
const selectableIds = computed<number[]>(() =>
  items.value.filter((row) => !row.conflict).map((row) => row.nodeId)
);

const columns = computed<DataTableColumns<ClassifyItemDto>>(() => [
  {
    type: "selection",
    width: 48,
    disabled: (row: ClassifyItemDto) => row.conflict,
  },
  {
    title: t("classifyColTitle"),
    key: "title",
    minWidth: 160,
    ellipsis: { tooltip: true },
  },
  {
    title: t("classifyColCurrent"),
    key: "currentPath",
    minWidth: 150,
    render: (row) => h("span", { style: "font-size:12px" }, row.currentPath),
  },
  {
    title: t("classifyColRule"),
    key: "matchedRulePattern",
    minWidth: 130,
    render: (row) => h(NText, { code: true }, { default: () => row.matchedRulePattern || row.matchedRuleId }),
  },
  {
    title: t("classifyColTarget"),
    key: "targetPath",
    minWidth: 160,
    render: (row) =>
      h(
        NText,
        { code: true, type: row.conflict ? "error" : "success" },
        { default: () => row.targetPath }
      ),
  },
  {
    title: t("classifyConflict"),
    key: "conflict",
    width: 90,
    render: (row) =>
      row.conflict
        ? h(NText, { type: "error", style: "font-size:12px" }, { default: () => t("classifyConflict") })
        : null,
  },
]);

async function load(): Promise<void> {
  loading.value = true;
  try {
    items.value = await api.classifyPreview(includeClassified.value, [...props.selectedIds]);
    // 默认不勾选，由用户显式选择要归类的项（冲突项始终不可勾选）。
    checkedIds.value = [];
  } catch (e) {
    items.value = [];
    checkedIds.value = [];
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    loading.value = false;
  }
}

function reload(): void {
  void load();
}

function selectAll(): void {
  checkedIds.value = [...selectableIds.value];
}

function onChecked(keys: Array<string | number>): void {
  checkedIds.value = keys.map((key) => Number(key));
}

async function apply(): Promise<void> {
  const chosen = new Set(checkedIds.value);
  const payload: ClassifyChoiceDto[] = items.value
    .filter((row) => !row.conflict && chosen.has(row.nodeId))
    .map((row) => ({ nodeId: row.nodeId, targetPath: row.targetPath }));
  if (payload.length === 0) return;

  applying.value = true;
  try {
    const count = await api.classifyApply(payload);
    message.success(t("classifyApplyOk", { n: count }));
    visible.value = false;
    emit("applied", count);
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    applying.value = false;
  }
}
</script>

<style scoped>
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
</style>
