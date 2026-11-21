<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('trashTitle')"
    style="width: 760px"
    @after-enter="load"
  >
    <n-data-table :columns="columns" :data="entries" :max-height="420" size="small" />
    <div v-if="entries.length === 0" class="empty">{{ t("trashEmpty") }}</div>
    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, h, ref } from "vue";
import { NButton, NDataTable, NModal, type DataTableColumns } from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { TrashEntry } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void; (e: "changed"): void }>();
const { t } = useI18n();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const entries = ref<TrashEntry[]>([]);

async function load() {
  try {
    entries.value = await api.trashList();
  } catch {
    entries.value = [];
  }
}

function fmtTime(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const columns = computed<DataTableColumns<TrashEntry>>(() => [
  { title: t("trashColTitle"), key: "title", ellipsis: { tooltip: true } },
  {
    title: t("trashColKind"),
    key: "kind",
    width: 80,
    render: (row) => h("span", row.kind === "folder" ? t("kindFolder") : t("kindBookmark")),
  },
  { title: t("trashColCount"), key: "itemCount", width: 80 },
  { title: t("trashColTime"), key: "time", width: 150, render: (row) => h("span", fmtTime(row.deletedAt)) },
  {
    title: "",
    key: "actions",
    width: 190,
    render: (row) =>
      h("div", { style: "display:flex; gap:6px; justify-content:flex-end" }, [
        h(
          NButton,
          {
            size: "tiny",
            type: "primary",
            ghost: true,
            onClick: async () => {
              await api.trashRestore(row.id);
              await load();
              emit("changed");
            },
          },
          { default: () => t("restore") }
        ),
        h(
          NButton,
          {
            size: "tiny",
            type: "error",
            ghost: true,
            onClick: async () => {
              await api.trashPurge(row.id);
              await load();
              emit("changed");
            },
          },
          { default: () => t("purge") }
        ),
      ]),
  },
]);
</script>

<style scoped>
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
</style>
