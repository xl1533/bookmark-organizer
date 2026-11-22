<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('incrementTitle')"
    style="width: 460px"
    @after-enter="load"
  >
    <n-alert type="info" :bordered="false" style="margin-bottom: 12px; font-size: 12px">
      {{ t("incrementHint") }}
    </n-alert>

    <n-spin :show="loading">
      <div class="count-grid">
        <div class="count-item"><span>{{ t("incrementNew") }}</span><b>{{ plan?.newCount ?? 0 }}</b></div>
        <div class="count-item">
          <span>{{ t("incrementUrlChanged") }}</span><b>{{ plan?.urlChangedCount ?? 0 }}</b>
        </div>
        <div class="count-item"><span>{{ t("incrementStale") }}</span><b>{{ plan?.staleCount ?? 0 }}</b></div>
        <div class="count-item"><span>{{ t("incrementSkipped") }}</span><b>{{ plan?.skippedCount ?? 0 }}</b></div>
      </div>
      <div class="total-line">{{ t("incrementTotal") }}: <b>{{ plan?.total ?? 0 }}</b></div>
      <div class="days-line">
        {{ t("incrementDays") }}: <b>{{ days }}</b>
        <n-button text type="primary" size="tiny" @click="goSettings">
          {{ t("incrementGoSettings") }}
        </n-button>
      </div>
      <div v-if="!loading && (plan?.total ?? 0) === 0" class="empty">{{ t("incrementEmpty") }}</div>
    </n-spin>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("cancel") }}</n-button>
        <n-button
          type="primary"
          :disabled="loading || (plan?.total ?? 0) === 0"
          :loading="starting"
          @click="confirm"
        >
          {{ t("incrementConfirm") }}
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { NAlert, NButton, NModal, NSpace, NSpin, useMessage } from "naive-ui";
import { useI18n } from "../i18n";
import { api, getIncrementalDays } from "../api";
import type { IncrementalPlanDto } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "update:show", v: boolean): void;
  (e: "started", total: number): void;
  (e: "goSettings"): void;
}>();
const { t } = useI18n();
const message = useMessage();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const plan = ref<IncrementalPlanDto | null>(null);
const days = ref(7);
const loading = ref(false);
const starting = ref(false);

/** 打开时读取设置中的 N 天并拉取预览；days 在对话框生命周期内锁定为快照。 */
async function load() {
  plan.value = null;
  loading.value = true;
  days.value = await getIncrementalDays();
  try {
    plan.value = await api.incrementalPreview(days.value);
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    loading.value = false;
  }
}

function goSettings() {
  visible.value = false;
  emit("goSettings");
}

/** 预览与检测使用同一份 days 快照；成功后关闭并把 total 交给 MainView 维护进度。 */
async function confirm() {
  starting.value = true;
  try {
    const res = await api.startIncrementalCheck(days.value);
    visible.value = false;
    emit("started", res.total);
  } catch (e) {
    message.error(`${t("opFailed")}: ${e}`);
  } finally {
    starting.value = false;
  }
}
</script>

<style scoped>
.count-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-bottom: 12px;
}
.count-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-radius: 6px;
  background: rgba(128, 128, 128, 0.1);
  font-size: 13px;
}
.count-item b {
  font-size: 16px;
}
.total-line,
.days-line {
  font-size: 13px;
  padding: 4px 2px;
}
.days-line {
  display: flex;
  align-items: center;
  gap: 8px;
}
.empty {
  text-align: center;
  opacity: 0.6;
  padding: 16px 0;
  font-size: 13px;
}
</style>
