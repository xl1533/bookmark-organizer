<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="isFuzzy ? t('fuzzyDedupTitle') : t('dedupTitle')"
    style="width: 820px"
  >
    <n-alert type="default" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ isFuzzy ? t("fuzzyHint") : t("dedupHint") }}
    </n-alert>
    <div v-if="isFuzzy" class="fuzzy-panel">
      <n-space :size="16" align="center" :wrap-item="false">
        <span class="opt-label">{{ t("optIgnoreQuery") }}</span>
        <n-switch
          v-model:value="opts.ignoreQuery"
          :disabled="applying || loading"
          @update:value="onOptionChange"
        />
        <span class="opt-label">{{ t("optIgnoreCase") }}</span>
        <n-switch
          v-model:value="opts.ignoreCase"
          :disabled="applying || loading"
          @update:value="onOptionChange"
        />
        <span class="opt-label">{{ t("optIgnoreTrailingSlash") }}</span>
        <n-switch
          v-model:value="opts.ignoreTrailingSlash"
          :disabled="applying || loading"
          @update:value="onOptionChange"
        />
        <span class="opt-label">{{ t("optIgnoreFragment") }}</span>
        <n-switch
          v-model:value="opts.ignoreFragment"
          :disabled="applying || loading"
          @update:value="onOptionChange"
        />
        <n-button size="tiny" quaternary :disabled="applying || loading" @click="restoreDefaults">
          {{ t("restoreDefaults") }}
        </n-button>
      </n-space>
      <div class="hint-row">
        <span v-if="opts.ignoreQuery" class="warn">{{ t("optIgnoreQueryHint") }}</span>
        <span>{{ t("mergeSuggestHint") }}</span>
      </div>
    </div>
    <n-data-table
      :columns="columns"
      :data="displayGroups"
      :loading="loading"
      :max-height="420"
      size="small"
    />
    <div v-if="displayGroups.length === 0 && !loading" class="empty">{{ t("dedupEmpty") }}</div>
    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("close") }}</n-button>
        <n-button
          type="error"
          :disabled="displayGroups.length === 0 || loading || applying"
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
import { computed, h, onBeforeUnmount, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NDataTable,
  NModal,
  NSpace,
  NSwitch,
  type DataTableColumns,
} from "naive-ui";
import { useI18n } from "../i18n";
import { api, DEFAULT_FUZZY_OPTIONS, getFuzzyOptions, setFuzzyOptions } from "../api";
import type { DupGroup, FuzzyOptions, ItemDto } from "../types";

const props = defineProps<{
  show: boolean;
  groups: DupGroup[];
  mode?: "exact" | "fuzzy";
}>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void; (e: "applied", n: number): void }>();
const { t } = useI18n();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const isFuzzy = computed(() => props.mode === "fuzzy");
const opts = ref<FuzzyOptions>({ ...DEFAULT_FUZZY_OPTIONS });
/** 最近一次真正用于预览的开关快照；应用只用它，绝不重读已变化的开关。 */
const appliedOpts = ref<FuzzyOptions | null>(null);
const fuzzyGroups = ref<DupGroup[]>([]);
const displayGroups = computed<DupGroup[]>(() => (isFuzzy.value ? fuzzyGroups.value : props.groups));
const loading = ref(false);
const applying = ref(false);

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let previewSeq = 0;

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

/** 用指定的 opts 快照重算预览；过期请求（seq 不匹配）的结果直接丢弃。 */
async function runFuzzyPreview(snapshot: FuzzyOptions, seq: number): Promise<void> {
  appliedOpts.value = { ...snapshot };
  loading.value = true;
  try {
    const result = await api.fuzzyDedupPreview(snapshot);
    if (seq === previewSeq) fuzzyGroups.value = result;
  } finally {
    if (seq === previewSeq) loading.value = false;
  }
}

/** 打开模糊去重：从 settings 回填开关，然后按回填值重算预览。 */
async function initFuzzy(): Promise<void> {
  const seq = ++previewSeq;
  fuzzyGroups.value = [];
  appliedOpts.value = null;
  const loaded = await getFuzzyOptions();
  if (seq !== previewSeq) return;
  opts.value = loaded;
  await runFuzzyPreview({ ...loaded }, seq);
}

/** 开关变化：立即持久化，防抖 250ms 重算预览。 */
function onOptionChange(): void {
  void setFuzzyOptions(opts.value);
  if (debounceTimer !== null) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    const seq = ++previewSeq;
    void runFuzzyPreview({ ...opts.value }, seq);
  }, 250);
}

async function restoreDefaults(): Promise<void> {
  opts.value = { ...DEFAULT_FUZZY_OPTIONS };
  await setFuzzyOptions(opts.value);
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  const seq = ++previewSeq;
  await runFuzzyPreview({ ...opts.value }, seq);
}

watch(
  () => [props.show, props.mode ?? "exact"] as const,
  ([show, mode]) => {
    if (!show) {
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
        debounceTimer = null;
      }
      previewSeq += 1;
      loading.value = false;
      return;
    }
    if (mode === "fuzzy") void initFuzzy();
  }
);

onBeforeUnmount(() => {
  if (debounceTimer !== null) clearTimeout(debounceTimer);
});

async function apply(): Promise<void> {
  if (applying.value || displayGroups.value.length === 0) return;
  applying.value = true;
  try {
    if (isFuzzy.value) {
      // 与最近一次预览完全相同的快照；切换开关期间预览按钮禁用，杜绝参数漂移。
      const snapshot: FuzzyOptions = appliedOpts.value
        ? { ...appliedOpts.value }
        : { ...DEFAULT_FUZZY_OPTIONS };
      const n = await api.fuzzyDedupApply(snapshot);
      emit("applied", n);
    } else {
      // 第 1 期精确去重入口保持原语义
      const n = await api.dedupApply();
      emit("applied", n);
    }
    visible.value = false;
  } finally {
    applying.value = false;
  }
}
</script>

<style scoped>
.fuzzy-panel {
  margin-bottom: 12px;
  padding: 8px 10px;
  border: 1px solid var(--n-border-color, #e0e0e0);
  border-radius: 6px;
}
.opt-label {
  font-size: 12px;
}
.hint-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 8px;
  font-size: 12px;
  opacity: 0.72;
}
.warn {
  color: #d03050;
}
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 24px 0;
}
</style>
