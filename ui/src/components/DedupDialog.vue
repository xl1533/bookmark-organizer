<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('dedupTitle')"
    style="width: 780px"
  >
    <n-alert type="default" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("dedupHint") }}
    </n-alert>
    <n-data-table
      :columns="columns"
      :data="groups"
      :max-height="420"
      size="small"
    />
    <div v-if="groups.length === 0" class="empty">{{ t("dedupEmpty") }}</div>
    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          type="error"
          :disabled="groups.length === 0"
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
import { NAlert, NButton, NDataTable, NModal, NSpace, type DataTableColumns } from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { DupGroup, ItemDto } from "../types";

const props = defineProps<{ show: boolean; groups: DupGroup[] }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void; (e: "applied", n: number): void }>();
const { t } = useI18n();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const applying = ref(false);

function pathText(item: ItemDto): string {
  return item.path ? `${item.path} / ${item.title}` : item.title;
}

function renderDupCount(row: DupGroup) {
  return h("span", String(row.duplicates.length));
}

const columns = computed<DataTableColumns<DupGroup>>(() => [
  {
    type: "expand",
    renderExpand: (row) =>
      h(
        "div",
        { style: "padding: 4px 0 8px 12px; font-size: 12px; line-height: 1.8" },
        row.duplicates.map((d) =>
          h("div", { key: d.id }, `#${d.id} ${d.title}（${pathText(d)}）`)
        )
      ),
  },
  {
    title: t("dedupColUrl"),
    key: "normalizedUrl",
    ellipsis: { tooltip: true },
    minWidth: 260,
  },
  {
    title: t("dedupColKeep"),
    key: "keep",
    render: (row) => h("span", { style: "font-size: 12px" }, `${row.keep.title}（${pathText(row.keep)}）`),
  },
  {
    title: t("dedupColDups"),
    key: "dups",
    width: 80,
    render: renderDupCount,
  },
]);

async function apply() {
  applying.value = true;
  try {
    const n = await api.dedupApply();
    emit("applied", n);
    visible.value = false;
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
