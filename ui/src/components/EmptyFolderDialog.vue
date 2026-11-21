<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('emptyTitle')"
    style="width: 640px"
  >
    <n-alert type="default" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("emptyHint") }}
    </n-alert>
    <n-data-table :columns="columns" :data="items" :max-height="380" size="small" />
    <div v-if="items.length === 0" class="empty">{{ t("emptyNone") }}</div>
    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          type="error"
          :disabled="items.length === 0"
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
import { computed, ref } from "vue";
import { NAlert, NButton, NDataTable, NModal, NSpace, type DataTableColumns } from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { ItemDto } from "../types";

const props = defineProps<{ show: boolean; items: ItemDto[] }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void; (e: "applied", n: number): void }>();
const { t } = useI18n();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const applying = ref(false);

const columns = computed<DataTableColumns<ItemDto>>(() => [
  { title: t("emptyColFolder"), key: "title", width: 220 },
  { title: t("emptyColPath"), key: "path", ellipsis: { tooltip: true } },
]);

async function apply() {
  applying.value = true;
  try {
    const ids = props.items.map((i) => i.id);
    const n = await api.deleteEmptyFolders(ids);
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
