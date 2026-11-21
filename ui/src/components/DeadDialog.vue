<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('deadTitle')"
    style="width: 860px"
    @after-enter="load"
  >
    <n-alert type="warning" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("deadBackupNote") }}
    </n-alert>

    <template v-if="done">
      <n-result status="success" :title="t('deadDeleted', { n: result?.deleted ?? 0 })" size="small">
        <template #default>
          <div class="backup-path">
            {{ t("deadBackupPath") }}:
            <n-text code>{{ result?.backupPath }}</n-text>
          </div>
        </template>
      </n-result>
    </template>

    <template v-else>
      <n-data-table :columns="columns" :data="items" :max-height="380" size="small" />
      <div v-if="items.length === 0" class="empty">{{ t("deadEmpty") }}</div>
    </template>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          v-if="!done"
          type="error"
          :disabled="items.length === 0"
          :loading="deleting"
          @click="apply"
        >
          {{ t("deadDeleteBtn") }}
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
  NSpace,
  NResult,
  NText,
  type DataTableColumns,
} from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { DeadItem, DeleteDeadResult } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void; (e: "deleted", n: number): void }>();
const { t } = useI18n();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const items = ref<DeadItem[]>([]);
const deleting = ref(false);
const done = ref(false);
const result = ref<DeleteDeadResult | null>(null);

async function load() {
  done.value = false;
  result.value = null;
  try {
    items.value = await api.listDead();
  } catch {
    items.value = [];
  }
}

const columns = computed<DataTableColumns<DeadItem>>(() => [
  { title: t("deadColTitle"), key: "title", width: 180, ellipsis: { tooltip: true } },
  { title: t("deadColUrl"), key: "url", ellipsis: { tooltip: true } },
  {
    title: t("deadColCode"),
    key: "httpCode",
    width: 70,
    render: (row) => h("span", row.httpCode != null ? String(row.httpCode) : "—"),
  },
  { title: t("deadColReason"), key: "message", ellipsis: { tooltip: true } },
  { title: t("deadColPath"), key: "path", width: 160, ellipsis: { tooltip: true } },
]);

async function apply() {
  deleting.value = true;
  try {
    const r = await api.deleteDead();
    result.value = r;
    done.value = true;
    emit("deleted", r.deleted);
  } finally {
    deleting.value = false;
  }
}
</script>

<style scoped>
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
.backup-path {
  font-size: 12px;
  word-break: break-all;
  text-align: left;
}
</style>
