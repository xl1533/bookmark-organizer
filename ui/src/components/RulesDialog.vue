<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('rulesTitle')"
    style="width: 980px"
    @after-enter="load"
  >
    <n-alert type="info" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      <div>{{ t("rulesHint") }}</div>
      <div>{{ t("rulesFirstMatchHint") }}</div>
    </n-alert>

    <n-data-table
      :columns="columns"
      :data="rules"
      :row-key="rowKey"
      :loading="loading"
      :max-height="420"
      size="small"
    />
    <div v-if="rules.length === 0 && !loading" class="empty">{{ t("rulesEmpty") }}</div>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button :loading="saving" @click="addRule">{{ t("rulesAdd") }}</n-button>
        <n-button type="primary" :disabled="rules.length === 0" @click="openPreview">
          {{ t("rulesPreview") }}
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
  NInput,
  NModal,
  NSelect,
  NSpace,
  NSwitch,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { api } from "../api";
import { useI18n } from "../i18n";
import type { Rule, RuleField, RuleInput } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "preview"): void;
}>();
const { t } = useI18n();
const message = useMessage();

const visible = computed({
  get: () => props.show,
  set: (value: boolean) => emit("update:show", value),
});

const rules = ref<Rule[]>([]);
const loading = ref(false);
const saving = ref(false);

function rowKey(row: Rule): string {
  return row.id;
}

const fieldOptions = computed<Array<{ label: string; value: RuleField }>>(() => [
  { label: t("rulesFieldHost"), value: "host" },
  { label: t("rulesFieldUrlKeyword"), value: "urlKeyword" },
  { label: t("rulesFieldTitleKeyword"), value: "titleKeyword" },
]);

const columns = computed<DataTableColumns<Rule>>(() => [
  {
    title: "#",
    key: "order",
    width: 52,
    render: (_row, index) => h("span", { style: "font-size:12px" }, String(index + 1)),
  },
  {
    title: t("rulesColField"),
    key: "field",
    width: 150,
    render: (row) =>
      h(NSelect, {
        size: "small",
        value: row.field,
        options: fieldOptions.value,
        "onUpdate:value": (value: RuleField) => {
          row.field = value;
          void saveRule(row);
        },
      }),
  },
  {
    title: t("rulesColPattern"),
    key: "pattern",
    minWidth: 190,
    render: (row) =>
      h(NInput, {
        size: "small",
        value: row.pattern,
        placeholder: t("rulesColPattern"),
        "onUpdate:value": (value: string) => {
          row.pattern = value;
        },
        onBlur: () => void saveRule(row),
      }),
  },
  {
    title: t("rulesColTarget"),
    key: "targetPath",
    minWidth: 190,
    render: (row) =>
      h(NInput, {
        size: "small",
        value: row.targetPath,
        placeholder: t("rulesColTarget"),
        "onUpdate:value": (value: string) => {
          row.targetPath = value;
        },
        onBlur: () => void saveRule(row),
      }),
  },
  {
    title: t("rulesColEnabled"),
    key: "enabled",
    width: 84,
    render: (row) =>
      h(NSwitch, {
        size: "small",
        value: row.enabled,
        "onUpdate:value": (value: boolean) => {
          row.enabled = value;
          void saveRule(row);
        },
      }),
  },
  {
    title: t("rulesColActions"),
    key: "actions",
    width: 230,
    render: (row, index) =>
      h(NSpace, { size: 4 }, {
        default: () => [
          h(
            NButton,
            {
              size: "tiny",
              disabled: index === 0,
              onClick: () => void moveRule(index, -1),
            },
            { default: () => t("rulesMoveUp") }
          ),
          h(
            NButton,
            {
              size: "tiny",
              disabled: index === rules.value.length - 1,
              onClick: () => void moveRule(index, 1),
            },
            { default: () => t("rulesMoveDown") }
          ),
          h(
            NButton,
            { size: "tiny", type: "error", ghost: true, onClick: () => void removeRule(row) },
            { default: () => t("rulesDelete") }
          ),
        ],
      }),
  },
]);

async function load(): Promise<void> {
  loading.value = true;
  try {
    rules.value = await api.ruleList();
  } catch (e) {
    rules.value = [];
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    loading.value = false;
  }
}

/** 编辑框失焦 / 下拉与开关变更后立即持久化，保证预览读到的始终是最新规则。 */
async function saveRule(rule: Rule): Promise<void> {
  const pattern = rule.pattern.trim();
  const targetPath = rule.targetPath.trim();
  if (!pattern) {
    message.warning(t("rulesPatternRequired"));
    await load();
    return;
  }
  if (!targetPath) {
    message.warning(t("rulesTargetRequired"));
    await load();
    return;
  }
  saving.value = true;
  try {
    await api.ruleUpdate({ ...rule, pattern, targetPath });
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
    await load();
  } finally {
    saving.value = false;
  }
}

/** 新增一条默认关闭的示例行，用户随后在表格内修改字段/值/目标路径。 */
async function addRule(): Promise<void> {
  const input: RuleInput = {
    field: "host",
    pattern: t("rulesNewPattern"),
    targetPath: t("rulesNewTarget"),
    enabled: false,
  };
  saving.value = true;
  try {
    const created = await api.ruleAdd(input);
    rules.value.push(created);
    message.success(t("rulesSaved"));
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    saving.value = false;
  }
}

async function moveRule(index: number, delta: number): Promise<void> {
  const target = index + delta;
  if (target < 0 || target >= rules.value.length) return;
  const next = [...rules.value];
  const [moved] = next.splice(index, 1);
  next.splice(target, 0, moved);
  rules.value = next;
  try {
    await api.ruleReorder(next.map((r) => r.id));
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
    await load();
  }
}

async function removeRule(rule: Rule): Promise<void> {
  try {
    await api.ruleDelete(rule.id);
    rules.value = rules.value.filter((r) => r.id !== rule.id);
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  }
}

function openPreview(): void {
  visible.value = false;
  emit("preview");
}
</script>

<style scoped>
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
</style>
