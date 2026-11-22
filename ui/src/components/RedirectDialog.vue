<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('redirectTitle')"
    style="width: 920px"
    @after-enter="load"
  >
    <n-alert type="warning" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("redirectBackupNote") }}
    </n-alert>

    <template v-if="done">
      <n-result
        status="success"
        :title="t('redirectApplyOk', { n: result?.updated ?? 0 })"
        size="small"
      >
        <template #default>
          <div class="backup-path">
            {{ t("redirectBackupPath") }}:
            <n-text code>{{ result?.backupPath }}</n-text>
          </div>
        </template>
      </n-result>
    </template>

    <template v-else>
      <n-space size="small" style="margin-bottom: 8px" align="center">
        <n-button size="tiny" :disabled="items.length === 0" @click="selectAll">
          {{ t("redirectSelectAll") }}
        </n-button>
        <n-button size="tiny" :disabled="items.length === 0" @click="invert">
          {{ t("redirectInvert") }}
        </n-button>
        <n-text depth="3" style="font-size: 12px">{{ t("redirectHint") }}</n-text>
      </n-space>
      <n-data-table
        :columns="columns"
        :data="items"
        :row-key="rowKey"
        :checked-row-keys="checkedIds"
        @update:checked-row-keys="onChecked"
        :max-height="380"
        size="small"
      />
      <div v-if="items.length === 0" class="empty">{{ t("redirectEmpty") }}</div>
    </template>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          v-if="!done"
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
  NModal,
  NResult,
  NSpace,
  NTag,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { useI18n } from "../i18n";
import { api } from "../api";
import type { RedirectApplyDto, RedirectDto } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "update:show", v: boolean): void;
  (e: "applied", n: number): void;
}>();
const { t } = useI18n();
const message = useMessage();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const items = ref<RedirectDto[]>([]);
const checkedIds = ref<number[]>([]);
const applying = ref(false);
const done = ref(false);
const result = ref<RedirectApplyDto | null>(null);

function rowKey(row: RedirectDto): number {
  return row.nodeId;
}

async function load() {
  done.value = false;
  result.value = null;
  checkedIds.value = [];
  try {
    items.value = await api.listRedirects();
    // 默认全部勾选，用户可逐条取消（支持全选/反选）
    checkedIds.value = items.value.map((r) => r.nodeId);
  } catch {
    items.value = [];
  }
}

function selectAll() {
  checkedIds.value = items.value.map((r) => r.nodeId);
}

function invert() {
  const selected = new Set(checkedIds.value);
  checkedIds.value = items.value
    .filter((r) => !selected.has(r.nodeId))
    .map((r) => r.nodeId);
}

function onChecked(keys: Array<string | number>) {
  checkedIds.value = keys.map((k) => Number(k));
}

const columns = computed<DataTableColumns<RedirectDto>>(() => [
  { type: "selection", width: 48 },
  { title: t("redirectColTitle"), key: "title", width: 160, ellipsis: { tooltip: true } },
  {
    title: `${t("redirectColOld")} → ${t("redirectColNew")}`,
    key: "oldUrl",
    render: (row) =>
      h("div", { class: "url-pair" }, [
        h(NText, { code: true, depth: 3 }, { default: () => row.oldUrl }),
        h("span", { class: "arrow" }, " → "),
        h(NText, { code: true, type: "success" }, { default: () => row.newUrl }),
      ]),
  },
  {
    title: t("redirectColStatus"),
    key: "status",
    width: 92,
    render: (row) =>
      h(
        NTag,
        {
          size: "small",
          bordered: false,
          type: row.status === "ok" ? "success" : "warning",
        },
        {
          default: () => (row.status === "ok" ? t("statusOk") : t("statusSuspicious")),
        }
      ),
  },
  {
    title: t("redirectColTime"),
    key: "checkedAt",
    width: 160,
    render: (row) => new Date(row.checkedAt * 1000).toLocaleString(),
  },
]);

async function apply() {
  applying.value = true;
  try {
    const r = await api.applyRedirects(checkedIds.value);
    result.value = r;
    done.value = true;
    emit("applied", r.updated);
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
.backup-path {
  font-size: 12px;
  word-break: break-all;
  text-align: left;
}
.url-pair {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.url-pair .arrow {
  flex: none;
  opacity: 0.6;
}
</style>
