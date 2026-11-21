<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('settingsTitle')"
    style="width: 560px"
    @after-enter="load"
  >
    <n-form label-placement="left" label-width="110">
      <n-form-item :label="t('settingsLanguage')">
        <n-select v-model:value="lang" style="width: 200px" :options="langOptions" />
      </n-form-item>
      <n-form-item :label="t('settingsTheme')">
        <n-radio-group v-model:value="theme">
          <n-radio-button value="auto">{{ t("themeAuto") }}</n-radio-button>
          <n-radio-button value="light">{{ t("themeLight") }}</n-radio-button>
          <n-radio-button value="dark">{{ t("themeDark") }}</n-radio-button>
        </n-radio-group>
      </n-form-item>
      <n-form-item :label="t('settingsProxy')">
        <n-input-group>
          <n-input
            v-model:value="proxy"
            :placeholder="t('proxyPlaceholder')"
            :disabled="testing"
            clearable
          />
          <n-button :loading="testing" @click="runTest">{{ t("proxyTest") }}</n-button>
        </n-input-group>
      </n-form-item>
      <n-form-item label=" " v-if="testResult">
        <n-text :type="testResult.ok ? 'success' : 'error'" style="font-size: 12px">
          {{ testResult.message }}
        </n-text>
      </n-form-item>
      <n-form-item :label="t('settingsDb')">
        <n-text code style="font-size: 11px; word-break: break-all">{{ dbPath }}</n-text>
      </n-form-item>
    </n-form>
    <n-alert type="info" :bordered="false" style="font-size: 12px">
      {{ t("checkDisclaimer") }}
    </n-alert>
    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("cancel") }}</n-button>
        <n-button type="primary" :loading="saving" @click="save">{{ t("proxySave") }}</n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  NAlert,
  NButton,
  NForm,
  NFormItem,
  NInput,
  NInputGroup,
  NModal,
  NRadioButton,
  NRadioGroup,
  NSelect,
  NSpace,
  NText,
} from "naive-ui";
import { useI18n, type Locale } from "../i18n";
import { useAppStore } from "../stores/app";
import { api } from "../api";
import type { ProxyTestResult } from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();
const { t, setLocale } = useI18n();
const store = useAppStore();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const lang = ref<Locale>("zh");
const theme = ref<"auto" | "light" | "dark">("auto");
const proxy = ref("");
const testing = ref(false);
const saving = ref(false);
const testResult = ref<ProxyTestResult | null>(null);
const dbPath = ref("");

const langOptions = [
  { label: "简体中文", value: "zh" },
  { label: "English", value: "en" },
];

async function load() {
  lang.value = (localStorage.getItem("bo.locale") as Locale) ?? "zh";
  theme.value = store.themeMode;
  testResult.value = null;
  try {
    proxy.value = await api.getProxy();
  } catch {
    proxy.value = "";
  }
  try {
    dbPath.value = await api.getDbPath();
  } catch {
    dbPath.value = "";
  }
}

async function runTest() {
  testing.value = true;
  testResult.value = null;
  try {
    testResult.value = await api.testProxy(proxy.value);
  } catch (e) {
    testResult.value = { ok: false, latencyMs: null, message: String(e) };
  } finally {
    testing.value = false;
  }
}

async function save() {
  saving.value = true;
  try {
    setLocale(lang.value);
    store.setThemeMode(theme.value);
    await api.setProxy(proxy.value.trim());
    await store.refresh();
    visible.value = false;
  } finally {
    saving.value = false;
  }
}
</script>
